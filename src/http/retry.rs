//! Retry classification and execution shared by every Elasticsearch/OpenSearch
//! request.
//!
//! Data-safety rule (inherited from upstream elasticdump-rs): scroll continuations
//! advance a server-side cursor, so they are retried only when the request
//! provably did not take effect (HTTP 429 or a connect-phase failure). Every
//! other request is idempotent and retried per the configured [`RetryOn`] set.

use std::fmt;
use std::future::Future;
use std::time::Duration;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use log::{debug, warn};
use tokio_util::sync::CancellationToken;

use super::client::SendError;

/// Sentinel error marking a task that stopped because the pipeline was
/// cancelled (Ctrl+C, closed stdout reader, size cap reached, or a failing
/// sibling task) rather than because it failed itself.
#[derive(Debug, Clone, Copy)]
pub struct DumpCancelled;

impl fmt::Display for DumpCancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("dump cancelled")
    }
}

impl std::error::Error for DumpCancelled {}

/// L-804b: a failure caused by a condition that may clear by itself (a cut or refused
/// connection, a timeout, a dropped body). Displays exactly its message; only its type is
/// new, so [`crate::exit_code`] can tell it from a permanent error (exit 4 instead of 1).
#[derive(Debug, Clone)]
pub struct Transient(pub String);

impl fmt::Display for Transient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// L-804b: the scroll or point-in-time context of a dump was lost (expired, node restart)
/// or a scroll continuation could not be retried safely. Nothing was committed; a dump from
/// the start can succeed. Transient for the exit code; `--restartOnContextLoss` restarts
/// scroll dumps into a file on it.
#[derive(Debug, Clone)]
pub struct ContextLoss(pub String);

impl fmt::Display for ContextLoss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// L-804b: part of the run is already committed in a way a rerun would duplicate (bulk
/// documents without `_id`), so a transient cause still exits 1, not 4.
#[derive(Debug, Clone)]
pub struct NotRerunSafe(pub String);

impl fmt::Display for NotRerunSafe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `true` when `error` is (or wraps) the [`DumpCancelled`] sentinel.
pub fn is_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<DumpCancelled>().is_some()
}

/// A non-2xx response, kept structured so callers can match on the status or
/// on the error type in the body (e.g. `resource_already_exists_exception`).
#[derive(Debug, Clone)]
pub struct EsHttpError {
    pub prefix: String,
    pub operation: String,
    pub status: u16,
    pub body: String,
    /// `scheme://host:port/path` of the request, without userinfo or query
    /// (empty when unknown). Only used to name the endpoint on 401/403.
    pub endpoint: String,
}

impl EsHttpError {
    /// L-809: the cluster refused a new scroll because `search.max_open_scroll_context` is
    /// reached. A rerun of the same dump opens the same number of contexts, so it is not
    /// retried and exits 1.
    pub fn is_scroll_context_limit(&self) -> bool {
        self.body.contains("max_open_scroll_context")
            || self.body.contains("too many scroll contexts")
    }
}

impl fmt::Display for EsHttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}Elasticsearch {} failed with HTTP {}: ",
            self.prefix, self.operation, self.status
        )?;
        if let Some(summary) = server_error_summary(&self.body) {
            write!(f, "{summary}; body: ")?;
        }
        if looks_like_html(&self.body) {
            write!(
                f,
                "an HTML page, not JSON: a proxy, load balancer or firewall in front of the cluster answered (first 200 bytes: {})",
                html_excerpt(self.body.as_bytes(), 200)
            )?;
        } else {
            f.write_str(&truncate_body(&self.body))?;
        }
        match self.status {
            401 => write!(
                f,
                " (authentication failed at {}: check the user and password for this endpoint, e.g. the URL credentials, --httpAuthFile or the --input-/--output- specific options; an expired or locked user also gives 401)",
                self.endpoint_or_operation()
            ),
            403 => write!(
                f,
                " (permission denied at {}: {})",
                self.endpoint_or_operation(),
                privilege_hint(&self.endpoint)
            ),
            400 if self.body.contains("max_result_window") => write!(
                f,
                " (--limit is larger than the index allows per page: index.max_result_window, 10000 by default; lower --limit)"
            ),
            _ if self.is_scroll_context_limit() => write!(
                f,
                " (the cluster's open scroll limit was reached: search.max_open_scroll_context, 500 by default; a scroll dump holds one context per shard and slice, so shards x --slices must stay below it, minus other open scrolls. Lower --slices, dump fewer indices per run, use a point-in-time dump (--searchAfter --pit, which opens no scroll contexts), or raise search.max_open_scroll_context)"
            ),
            400 if self.body.contains("max_slices_per_scroll") => write!(
                f,
                " (--slices is larger than the index allows: index.max_slices_per_scroll, 1024 by default; lower --slices)"
            ),
            // L-804a: a final 429 means the cluster kept rejecting through every retry (429 is
            // retried by default); the raw es_rejected_execution_exception alone does not say
            // what to change.
            429 => write!(
                f,
                " (the cluster kept rejecting requests as overloaded (HTTP 429) through every retry: raise --retryAttempts / --retryDelay, or lower --concurrency, --limit or --maxSockets)"
            ),
            503 if self.body.contains("missing shards")
                || self.body.contains("no_shard_available_action_exception")
                || self.body.contains("all shards failed") =>
            {
                write!(
                    f,
                    " (shards are unavailable: the index is red, e.g. an unassigned primary after a lost node or an allocation filter; check GET _cluster/health/<index>?level=shards and re-run once it is yellow or green; partial search results are never accepted, they would silently miss documents)"
                )
            }
            _ => Ok(()),
        }
    }
}

/// An HTML document (proxy error pages, WAF blocks, login pages; TST-BRK-NET-010).
fn looks_like_html(body: &str) -> bool {
    let head: String = body
        .trim_start()
        .chars()
        .take(64)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype html")
        || head.starts_with("<html")
        || head.starts_with("<head")
        || head.starts_with("<body")
        || (head.starts_with('<') && body.to_ascii_lowercase().contains("</html>"))
}

/// The first `n` bytes of a body on one line (control characters and runs of whitespace
/// collapsed), with the `<title>` first when there is one.
fn html_excerpt(body: &[u8], n: usize) -> String {
    let text = String::from_utf8_lossy(body);
    let lower = text.to_ascii_lowercase();
    let title = lower.find("<title>").and_then(|i| {
        let rest = &text[i + 7..];
        rest.find("</").map(|j| rest[..j].trim().to_string())
    });
    let flat: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let head = truncate_at(&flat, n);
    let more = if head.len() < flat.len() { "…" } else { "" };
    match title {
        Some(t) if !t.is_empty() => format!("title \"{}\"; {head}{more}", truncate_at(&t, 100)),
        _ => format!("{head}{more}"),
    }
}

/// Largest part of a server body quoted in an error message (TST-API-056).
pub const MAX_ERROR_BODY: usize = 2048;

/// `type: reason` of an Elasticsearch/OpenSearch error body (`error.type` and
/// `error.reason`, plus `caused_by` when it adds something).
fn server_error_summary(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let e = v.get("error")?;
    let ty = e.get("type")?.as_str()?;
    let mut out = format!(
        "{ty}: {}",
        e.get("reason").and_then(|r| r.as_str()).unwrap_or("")
    );
    if let Some(c) = e.get("caused_by")
        && let (Some(cty), Some(cr)) = (
            c.get("type").and_then(|x| x.as_str()),
            c.get("reason").and_then(|x| x.as_str()),
        )
    {
        out.push_str(&format!(" (caused by {cty}: {cr})"));
    }
    if out.len() > MAX_ERROR_BODY / 2 {
        out = truncate_at(&out, MAX_ERROR_BODY / 2).to_string() + "…";
    }
    Some(out)
}

fn truncate_at(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

/// The body, cut at [`MAX_ERROR_BODY`] bytes with a note of how much was dropped.
fn truncate_body(body: &str) -> std::borrow::Cow<'_, str> {
    if body.len() <= MAX_ERROR_BODY {
        return body.into();
    }
    let head = truncate_at(body, MAX_ERROR_BODY);
    format!("{head}… ({} more bytes)", body.len() - head.len()).into()
}

/// The privilege a request path needs, for 403 messages (TST-API-054).
pub fn privilege_hint(endpoint: &str) -> &'static str {
    let path = endpoint
        .split_once("://")
        .map(|(_, rest)| rest.find('/').map_or("/", |i| &rest[i..]))
        .unwrap_or(endpoint);
    let has = |n: &str| path.contains(n);
    if path == "/" || path.is_empty() || has("/_cluster/state") || has("/_cluster/health") {
        "needs the 'monitor' cluster privilege"
    } else if has("/_plugins/_ism") || has("/_opendistro/_ism") {
        "needs the OpenSearch ISM permissions (cluster:admin/opendistro/ism/*, e.g. the ism_full_access role)"
    } else if has("/_ilm") {
        "needs the 'manage_ilm' cluster privilege ('read_ilm' is enough to dump)"
    } else if has("_index_template") || has("_component_template") || has("/_template") {
        "needs the 'manage_index_templates' cluster privilege"
    } else if has("/_scripts") {
        "needs the 'manage' cluster privilege to store scripts"
    } else if has("/_bulk") {
        "needs the 'write' (or 'index'/'create_doc') index privilege on the target index"
    } else if has("/_search/scroll") {
        "needs the 'read' index privilege on the source index (OpenSearch: also the cluster permission indices:data/read/scroll*, e.g. cluster_composite_ops_ro)"
    } else if has("_search") || has("/_pit") || has("point_in_time") || has("/_count") {
        "needs the 'read' index privilege on the source index"
    } else if has("/_mapping") || has("/_settings") || has("/_alias") || has("/_analyze") {
        "needs the 'manage' index privilege on the index ('view_index_metadata' is enough to dump)"
    } else if has("/_refresh") {
        "needs the 'maintenance' (or 'manage') index privilege on the target index"
    } else {
        "needs the 'create_index' or 'manage' index privilege on the index"
    }
}

impl std::error::Error for EsHttpError {}

impl EsHttpError {
    fn endpoint_or_operation(&self) -> &str {
        if self.endpoint.is_empty() {
            &self.operation
        } else {
            &self.endpoint
        }
    }

    pub fn error_type_is(&self, ty: &str) -> bool {
        self.body.contains(&format!("\"type\":\"{ty}\""))
    }
}

/// How the delay grows between attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Backoff {
    /// Same delay every time (elasticdump/requestretry behaviour).
    #[default]
    Fixed,
    /// Doubling delay capped at [`MAX_RETRY_DELAY`].
    Exponential,
}

/// Which failures are retryable for idempotent requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryOn {
    pub network: bool,
    pub timeout: bool,
    pub any_5xx: bool,
    pub s429: bool,
    pub s408: bool,
    /// Additional explicit statuses (0 = unused slot).
    pub extra: [u16; 8],
}

impl Default for RetryOn {
    /// elasticdump's `HTTPOrNetworkError` strategy (network errors and 5xx),
    /// plus 429/408, which elasticdump does not retry but which are the most
    /// common transient rejections.
    fn default() -> Self {
        Self {
            network: true,
            timeout: true,
            any_5xx: true,
            s429: true,
            s408: true,
            extra: [0; 8],
        }
    }
}

impl RetryOn {
    /// Parse a `--retryOn` list: `network,timeout,5xx,429,408,<status>...`.
    pub fn parse(spec: &str) -> Result<Self> {
        let mut on = Self {
            network: false,
            timeout: false,
            any_5xx: false,
            s429: false,
            s408: false,
            extra: [0; 8],
        };
        let mut n_extra = 0;
        for raw in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            match raw.to_ascii_lowercase().as_str() {
                "network" => on.network = true,
                "timeout" => on.timeout = true,
                "5xx" => on.any_5xx = true,
                "429" => on.s429 = true,
                "408" => on.s408 = true,
                other => {
                    let status: u16 = other
                        .parse()
                        .map_err(|_| anyhow!("--retryOn: unknown condition '{raw}'"))?;
                    if !(100..600).contains(&status) {
                        return Err(anyhow!("--retryOn: '{raw}' is not an HTTP status"));
                    }
                    if n_extra == on.extra.len() {
                        return Err(anyhow!("--retryOn: at most 8 explicit statuses"));
                    }
                    on.extra[n_extra] = status;
                    n_extra += 1;
                }
            }
        }
        Ok(on)
    }

    pub fn status(&self, status: u16) -> bool {
        (self.any_5xx && (500..600).contains(&status))
            || (self.s429 && status == 429)
            || (self.s408 && status == 408)
            || self.extra.contains(&status)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Extra attempts after the first one; 0 disables retries.
    pub attempts: usize,
    /// Delay before the first retry.
    pub base_delay: Duration,
    pub backoff: Backoff,
    pub retry_on: RetryOn,
    /// Largest response body accepted (`--maxResponseMb`); `None` = unlimited.
    pub max_response_bytes: Option<usize>,
}

/// Default response size cap: 256 MiB (TST-BRK-SRV-006).
pub const DEFAULT_MAX_RESPONSE_MB: usize = 256;

impl RetryPolicy {
    pub fn none() -> Self {
        Self {
            attempts: 0,
            base_delay: Duration::ZERO,
            backoff: Backoff::Fixed,
            retry_on: RetryOn::default(),
            max_response_bytes: Some(DEFAULT_MAX_RESPONSE_MB << 20),
        }
    }

    pub fn delay_for(&self, attempt: usize) -> Duration {
        match self.backoff {
            Backoff::Fixed => self.base_delay,
            Backoff::Exponential => {
                let exponent = u32::try_from(attempt).unwrap_or(u32::MAX);
                self.base_delay
                    .saturating_mul(2u32.saturating_pow(exponent))
                    .min(MAX_RETRY_DELAY)
            }
        }
    }
}

pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);

/// How a request may be retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryMode {
    /// Safe to repeat: PIT open/pages, initial searches, bulk, metadata calls.
    Idempotent,
    /// Scroll continuations: retry only on HTTP 429 or a connect failure.
    RejectionOnly,
}

/// Classified failure of one send attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryOutcome {
    /// The connection could not be established; the server never saw it.
    ConnectError,
    /// The request timed out; the server may or may not have processed it.
    Timeout,
    /// Any other transport failure (reset, broken pipe, body transfer).
    TransportError,
    /// TLS verification or negotiation failed: permanent, never retried.
    Tls,
    /// The server answered with this non-2xx status.
    Status(u16),
}

pub fn should_retry(policy: &RetryPolicy, mode: RetryMode, outcome: &RetryOutcome) -> bool {
    let on = &policy.retry_on;
    match mode {
        RetryMode::Idempotent => match outcome {
            RetryOutcome::ConnectError | RetryOutcome::TransportError => on.network,
            RetryOutcome::Tls => false,
            RetryOutcome::Timeout => on.timeout,
            RetryOutcome::Status(status) => on.status(*status),
        },
        RetryMode::RejectionOnly => match outcome {
            RetryOutcome::ConnectError => on.network,
            RetryOutcome::Status(429) => on.s429 || on.status(429),
            _ => false,
        },
    }
}

/// A plain-language explanation for TLS failures (TST-API-055), and whether
/// the failure is permanent (a retry cannot fix it).
pub fn tls_hint(chain: &str, plain_http: bool) -> Option<(bool, &'static str)> {
    let has = |n: &str| chain.contains(n);
    Some(if has("UnknownIssuer") {
        (
            true,
            "TLS: the server certificate is not signed by a trusted CA; pass the cluster's CA with --input-ca/--output-ca (or --ca), or --insecure to skip verification (unsafe)",
        )
    } else if has("certificate expired") || has("Expired") {
        (
            true,
            "TLS: the server certificate has expired; renew it, or pass --insecure to skip verification (unsafe)",
        )
    } else if has("not valid yet") || has("NotValidYet") {
        (
            true,
            "TLS: the server certificate is not valid yet; check the clocks of this machine and the server, or pass --insecure to skip verification (unsafe)",
        )
    } else if has("not valid for name") || has("NotValidForName") {
        (
            true,
            "TLS: hostname mismatch; connect with a host name or IP address listed in the certificate's subjectAltName, or pass --insecure to skip verification (unsafe)",
        )
    } else if has("CertificateRequired") {
        (
            true,
            "TLS: the server requires a client certificate; pass --input-cert/--input-key (or --cert/--key)",
        )
    } else if has("received fatal alert")
        && [
            "UnknownCA",
            "BadCertificate",
            "CertificateUnknown",
            "UnsupportedCertificate",
            "CertificateExpired",
            "CertificateRevoked",
            "AccessDenied",
        ]
        .iter()
        .any(|a| has(a))
    {
        (
            true,
            "TLS: the server rejected this client certificate: it must be signed by a CA the cluster trusts, be valid now and allow client authentication; check --input-cert/--output-cert (or --cert)",
        )
    } else if has("ProtocolVersion") || has("PeerIncompatible") || has("peer is incompatible") {
        (
            true,
            "TLS: the server only offers a TLS version or cipher this tool does not support (TLS 1.2+ is required)",
        )
    } else if has("InvalidContentType") || has("corrupt message") {
        (
            true,
            "TLS: the server did not answer with TLS; this port probably serves plain HTTP, so use http://",
        )
    } else if has("BadSignature") || has("UnsupportedSignatureAlgorithm") {
        (
            true,
            "TLS: the server certificate uses a key or signature algorithm this tool does not accept (an RSA key under 2048 bits, or SHA-1); re-issue it with RSA-2048+ or ECDSA and SHA-256",
        )
    } else if has("invalid peer certificate") || has("BadCertificate") || has("UnknownCA") {
        (
            true,
            "TLS: certificate verification failed; pass the cluster's CA with --input-ca/--output-ca (or --ca), or --insecure to skip verification (unsafe)",
        )
    } else if plain_http
        && (has("connection closed before message completed") || has("invalid HTTP version"))
    {
        (false, "if this port serves HTTPS, use https://")
    } else {
        return None;
    })
}

/// A body read that failed because the connection stalled or closed (L-804b), not because
/// the bytes did not decode.
fn body_cut_off(e: &reqwest::Error, chain: &str) -> bool {
    e.is_timeout()
        || [
            "end of file",
            "connection reset",
            "Connection reset",
            "connection closed",
            "broken pipe",
            "Broken pipe",
            "timed out",
            "unexpected EOF",
            "UnexpectedEof",
        ]
        .iter()
        .any(|n| chain.contains(n))
}

enum ReadError {
    TooLarge(usize),
    Http(reqwest::Error),
}

/// Read the body, refusing more than `cap` bytes (by Content-Length up front, or while
/// streaming) so a huge page or an endless body cannot exhaust memory (TST-BRK-SRV-006).
/// A body read error after the server answered is a transport error: the server already
/// processed the request, so scroll continuations never retry it.
async fn read_capped(
    mut response: reqwest::Response,
    cap: Option<usize>,
) -> std::result::Result<Bytes, ReadError> {
    let Some(cap) = cap else {
        return response.bytes().await.map_err(ReadError::Http);
    };
    if response.content_length().is_some_and(|n| n > cap as u64) {
        return Err(ReadError::TooLarge(cap));
    }
    let mut buf = bytes::BytesMut::new();
    while let Some(chunk) = response.chunk().await.map_err(ReadError::Http)? {
        if buf.len() + chunk.len() > cap {
            return Err(ReadError::TooLarge(cap));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf.freeze())
}

/// Hosts that answered at least once in this run (any HTTP status): their names resolved.
static ANSWERED_HOSTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn note_host_answered(url: &url::Url) {
    if let Some(h) = url.host_str() {
        let mut g = ANSWERED_HOSTS.lock().unwrap_or_else(|p| p.into_inner());
        if !g.iter().any(|x| x == h) {
            g.push(h.to_string());
        }
    }
}

/// A failed DNS lookup (TST-BRK-NET-008): `Some((permanent, message))`. "No such host" for a
/// host that never answered in this run is permanent (a typo or a wrong URL: fail fast, no
/// retries); a temporary resolver failure, or any lookup failure for a host that answered
/// before (DNS failing mid-run), is retried. The message names the host, never credentials.
fn dns_failure(error: &reqwest::Error, chain: &str) -> Option<(bool, String)> {
    if !chain.contains("dns error") {
        return None;
    }
    let host = error
        .url()
        .and_then(|u| u.host_str())
        .unwrap_or("?")
        .to_string();
    let answered = ANSWERED_HOSTS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains(&host);
    let no_such_host = [
        "Name or service not known",
        "No address associated",
        "nodename nor servname",
        "No such host",
        "not known",
    ]
    .iter()
    .any(|n| chain.contains(n));
    let cause = chain.rsplit("dns error: ").next().unwrap_or(chain);
    Some(if no_such_host && !answered {
        (
            true,
            format!(
                "cannot resolve host '{host}' ({cause}); check the host name in the URL (not retried)"
            ),
        )
    } else if answered {
        (
            false,
            format!(
                "DNS lookup of '{host}' failed ({cause}); the host resolved earlier in this run, so this is retried"
            ),
        )
    } else {
        (
            false,
            format!(
                "DNS lookup of '{host}' failed ({cause}); retried as a temporary resolver failure"
            ),
        )
    })
}

/// Which limit a timeout hit (TST-BRK-NET-004).
fn timeout_hint(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        if error.is_connect() {
            " (timed out while connecting: the DNS lookup or TCP connect took longer than --timeout ms / --idleTimeout seconds, default 300)"
        } else {
            " (timed out: no data from the server within --timeout ms / --idleTimeout seconds, default 300, or the whole request exceeded --requestTimeout)"
        }
    } else {
        ""
    }
}

/// Reconnects allowed per request when a new connection closes before the
/// request is written on it (see [`never_sent`]).
const MAX_REDIALS: usize = 3;

/// hyper's "connection was not ready": a *new* connection died before the
/// request was written on it (hyper re-sends on reused pooled connections
/// itself), so nothing was sent and the cause is not in this error. The usual
/// cause is TLS 1.3 client-certificate rejection: the handshake completes on
/// the client side, then the server's `CertificateRequired` / `BadCertificate`
/// alert closes the connection before the request is queued. Classifying that
/// as a transport failure retried it with a "retrying" warning (L-431a), and
/// the message never named the TLS cause. Reconnecting once (silently, without
/// using a retry attempt or the retry delay) gets the alert on the request
/// itself, which [`tls_hint`] then classifies as permanent.
fn never_sent(chain: &str) -> bool {
    chain.contains("connection was not ready")
}

fn classify(error: &reqwest::Error) -> RetryOutcome {
    if error.is_connect() {
        RetryOutcome::ConnectError
    } else if error.is_timeout() {
        RetryOutcome::Timeout
    } else {
        RetryOutcome::TransportError
    }
}

/// `error: source: source...`, so TLS and connect failures name their cause
/// (reqwest's own Display stops at "error sending request").
pub fn error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = s.source();
    }
    out
}

fn prefix_for(slice_id: Option<usize>) -> String {
    match slice_id {
        Some(id) => format!("Slice {id}: "),
        None => String::new(),
    }
}

/// Send a request built by `build_and_send`, validate the status, and retry
/// per `mode`. Cancellation (checked before each attempt and while sleeping)
/// yields [`DumpCancelled`].
pub async fn send_checked_with_retry<F, Fut>(
    retry: RetryPolicy,
    cancel: &CancellationToken,
    mode: RetryMode,
    slice_id: usize,
    operation: &str,
    build_and_send: F,
) -> Result<Bytes>
where
    F: Fn() -> Fut,
    Fut: Future<Output = std::result::Result<reqwest::Response, SendError>>,
{
    send_inner(
        retry,
        cancel,
        mode,
        Some(slice_id),
        operation,
        build_and_send,
    )
    .await
}

/// Same as [`send_checked_with_retry`] without a slice prefix in messages.
pub async fn send_with_retry<F, Fut>(
    retry: RetryPolicy,
    cancel: &CancellationToken,
    mode: RetryMode,
    operation: &str,
    build_and_send: F,
) -> Result<Bytes>
where
    F: Fn() -> Fut,
    Fut: Future<Output = std::result::Result<reqwest::Response, SendError>>,
{
    send_inner(retry, cancel, mode, None, operation, build_and_send).await
}

async fn send_inner<F, Fut>(
    retry: RetryPolicy,
    cancel: &CancellationToken,
    mode: RetryMode,
    slice_id: Option<usize>,
    operation: &str,
    build_and_send: F,
) -> Result<Bytes>
where
    F: Fn() -> Fut,
    Fut: Future<Output = std::result::Result<reqwest::Response, SendError>>,
{
    let prefix = prefix_for(slice_id);
    let mut attempt: usize = 0;
    let mut redials: usize = 0;
    loop {
        if cancel.is_cancelled() {
            return Err(anyhow::Error::new(DumpCancelled));
        }

        let mut retry_after: Option<Duration> = None;
        let (outcome, error) = match build_and_send().await {
            Ok(response) => {
                note_host_answered(response.url());
                let status = response.status();
                let endpoint = public_endpoint(response.url());
                let content_type = response
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(status.as_u16(), 429 | 503) {
                    retry_after = parse_retry_after(response.headers());
                }
                if status.is_redirection() {
                    let location = response
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .and_then(|v| v.to_str().ok())
                        .map(|l| {
                            response
                                .url()
                                .join(l)
                                .map(|u| public_endpoint(&u))
                                .unwrap_or_else(|_| truncate_at(l, 200).to_string())
                        })
                        .unwrap_or_else(|| "(no Location header)".to_string());
                    return Err(anyhow!(
                        "{prefix}{operation}: {endpoint} answered with a redirect (HTTP {}) to {location}; redirects are not followed for API calls (they could send credentials or documents elsewhere). Use the final URL directly",
                        status.as_u16()
                    ));
                }
                match read_capped(response, retry.max_response_bytes).await {
                    Err(ReadError::TooLarge(cap)) => {
                        // permanent: the same request would return the same body
                        return Err(anyhow!(
                            "{prefix}{operation} response from {endpoint} is larger than {} MB (--maxResponseMb); it was not read further. For searches lower --limit (documents per page) or select fewer fields; or raise --maxResponseMb (0 = unlimited) if the memory allows",
                            cap >> 20
                        ));
                    }
                    Err(ReadError::Http(e)) => {
                        let chain = error_chain(&e);
                        let msg = format!(
                            "{prefix}Failed to read {operation} response bytes: {chain}{}",
                            timeout_hint(&e)
                        );
                        // L-804b: a body cut off or stalled mid-transfer may not happen again; a body
                        // that does not decode (broken gzip/deflate/zstd) will
                        (
                            RetryOutcome::TransportError,
                            if body_cut_off(&e, &chain) {
                                anyhow::Error::msg(Transient(msg))
                            } else {
                                anyhow!(msg)
                            },
                        )
                    }
                    Ok(bytes) if status.is_success() && content_type.contains("text/html") => {
                        // permanent: a login page or proxy answered, not the cluster (BRK-NET-010)
                        return Err(anyhow!(
                            "{prefix}{operation}: expected JSON from {endpoint}, got text/html (HTTP {}; first 200 bytes: {}). A proxy, load balancer or login page answered instead of the cluster: check the URL, the proxy environment (--noProxy) and the authentication",
                            status.as_u16(),
                            html_excerpt(&bytes, 200)
                        ));
                    }
                    Ok(bytes) if status.is_success() => return Ok(bytes),
                    Ok(bytes) => {
                        let http = EsHttpError {
                            prefix: prefix.clone(),
                            operation: operation.to_string(),
                            status: status.as_u16(),
                            body: String::from_utf8_lossy(bytes.as_ref()).into_owned(),
                            endpoint,
                        };
                        if http.is_scroll_context_limit() {
                            return Err(anyhow::Error::new(http));
                        }
                        (
                            RetryOutcome::Status(status.as_u16()),
                            anyhow::Error::new(http),
                        )
                    }
                }
            }
            // Refused before sending (unsafe path): never retried.
            Err(SendError::InvalidPath(m)) => {
                return Err(anyhow!("{prefix}{operation}: {m}"));
            }
            Err(SendError::Http(e)) => {
                let chain = error_chain(&e);
                if never_sent(&chain) && redials < MAX_REDIALS {
                    // Not a failed attempt: the request never left, and the
                    // next connection surfaces the real cause (L-431a).
                    redials += 1;
                    debug!(
                        "{prefix}{operation}: a new connection closed before the request was sent ({chain}); reconnecting ({redials} of {MAX_REDIALS})"
                    );
                    continue;
                }
                if let Some((permanent, msg)) = dns_failure(&e, &chain) {
                    let msg = format!("{prefix}Elasticsearch {operation} request failed: {msg}");
                    if permanent {
                        return Err(anyhow!(msg));
                    }
                    (
                        RetryOutcome::ConnectError,
                        anyhow::Error::msg(Transient(msg)),
                    )
                } else {
                    let http = e.url().is_some_and(|u| u.scheme() == "http");
                    let hint = tls_hint(&chain, http);
                    // L-804b: a TLS or protocol mismatch, or a local file-descriptor limit, needs a
                    // change before a rerun can work; a closed connection alone may be transient
                    let needs_change = hint.is_some_and(|(permanent, _)| permanent)
                        || (http && chain.contains("invalid HTTP version"))
                        || chain.contains("Too many open files")
                        || chain.contains("os error 24");
                    let outcome = if hint.is_some_and(|(permanent, _)| permanent) {
                        RetryOutcome::Tls
                    } else {
                        classify(&e)
                    };
                    let hint = hint.map(|(_, h)| format!(" ({h})")).unwrap_or_else(|| {
                        if chain.contains("Connection refused") {
                            " (connection refused: the cluster is down or restarting, or the URL's port is wrong; --retryAttempts/--retryDelay set how long to wait for it)".to_string()
                        } else {
                            timeout_hint(&e).to_string()
                        }
                    });
                    let msg =
                        format!("{prefix}Elasticsearch {operation} request failed: {chain}{hint}");
                    // L-804b: every other transport failure (refused, reset, timeout, body cut)
                    // may clear by itself
                    let error = if outcome == RetryOutcome::Tls || needs_change {
                        anyhow!(msg)
                    } else {
                        anyhow::Error::msg(Transient(msg))
                    };
                    (outcome, error)
                }
            }
        };

        if attempt >= retry.attempts || !should_retry(&retry, mode, &outcome) {
            return Err(error);
        }

        // A server-sent `Retry-After` (seconds) on 429/503 is honoured as a
        // minimum delay, capped at MAX_RETRY_DELAY.
        let delay = retry
            .delay_for(attempt)
            .max(retry_after.unwrap_or_default().min(MAX_RETRY_DELAY));
        warn!(
            "{prefix}{operation} failed (attempt {} of {}), retrying in {:?}: {:#}",
            attempt + 1,
            retry.attempts + 1,
            delay,
            error
        );

        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = cancel.cancelled() => return Err(anyhow::Error::new(DumpCancelled)),
        }
        attempt += 1;
    }
}

/// `scheme://host[:port]/path`: never userinfo or query (TST-API-054, STD-ACC 5).
fn public_endpoint(url: &url::Url) -> String {
    let host = url.host_str().unwrap_or("?");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}{}", url.scheme(), url.path()),
        None => format!("{}://{host}{}", url.scheme(), url.path()),
    }
}

/// `Retry-After` in delay-seconds form (the HTTP-date form is ignored).
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let v = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    v.trim().parse::<u64>().ok().map(Duration::from_secs)
}

pub fn validate_response_bytes(
    status: u16,
    response_bytes: Bytes,
    prefix: &str,
    operation: &str,
) -> Result<Bytes> {
    if !(200..300).contains(&status) {
        return Err(anyhow::Error::new(EsHttpError {
            prefix: prefix.to_string(),
            operation: operation.to_string(),
            status,
            body: String::from_utf8_lossy(response_bytes.as_ref()).into_owned(),
            endpoint: String::new(),
        }));
    }
    Ok(response_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // L-431a: only hyper's "request never left" cancel is redialled; a request that failed
    // on the wire (sent or partly sent) is never re-sent silently.
    #[test]
    fn only_never_sent_requests_are_redialled() {
        assert!(never_sent(
            "error sending request for url (https://h/): client error (Canceled): operation was canceled: connection was not ready"
        ));
        for chain in [
            "error sending request for url (https://h/): client error (SendRequest): connection error: received fatal alert: CertificateRequired",
            "error sending request for url (http://h/): client error (SendRequest): connection closed before message completed",
            "error sending request for url (http://h/): client error (Connect): tcp connect error: Connection refused (os error 111)",
            "error sending request for url (http://h/): operation timed out",
        ] {
            assert!(!never_sent(chain), "{chain}");
        }
    }

    fn policy() -> RetryPolicy {
        RetryPolicy {
            attempts: 3,
            base_delay: Duration::from_millis(100),
            backoff: Backoff::Fixed,
            retry_on: RetryOn::default(),
            max_response_bytes: None,
        }
    }

    #[test]
    fn idempotent_requests_retry_network_5xx_429_408() {
        let p = policy();
        for o in [
            RetryOutcome::ConnectError,
            RetryOutcome::TransportError,
            RetryOutcome::Timeout,
            RetryOutcome::Status(500),
            RetryOutcome::Status(502),
            RetryOutcome::Status(503),
            RetryOutcome::Status(504),
            RetryOutcome::Status(429),
            RetryOutcome::Status(408),
        ] {
            assert!(should_retry(&p, RetryMode::Idempotent, &o), "{o:?}");
        }
        for s in [400, 401, 403, 404, 409] {
            assert!(!should_retry(
                &p,
                RetryMode::Idempotent,
                &RetryOutcome::Status(s)
            ));
        }
    }

    #[test]
    fn scroll_continuations_retry_only_when_provably_not_applied() {
        let p = policy();
        assert!(should_retry(
            &p,
            RetryMode::RejectionOnly,
            &RetryOutcome::Status(429)
        ));
        assert!(should_retry(
            &p,
            RetryMode::RejectionOnly,
            &RetryOutcome::ConnectError
        ));
        for o in [
            RetryOutcome::Timeout,
            RetryOutcome::TransportError,
            RetryOutcome::Status(500),
            RetryOutcome::Status(503),
        ] {
            assert!(!should_retry(&p, RetryMode::RejectionOnly, &o), "{o:?}");
        }
    }

    #[test]
    fn retry_on_parses_lists() {
        let on = RetryOn::parse("network,503").unwrap();
        assert!(on.network && !on.timeout && !on.any_5xx && !on.s429);
        assert!(on.status(503) && !on.status(502) && !on.status(429));
        assert!(RetryOn::parse("bogus").is_err());
        assert!(RetryOn::parse("999").is_err());
    }

    #[test]
    fn backoff_fixed_and_exponential() {
        let mut p = policy();
        assert_eq!(p.delay_for(0), Duration::from_millis(100));
        assert_eq!(p.delay_for(5), Duration::from_millis(100));
        p.backoff = Backoff::Exponential;
        assert_eq!(p.delay_for(3), Duration::from_millis(800));
        assert_eq!(p.delay_for(30), MAX_RETRY_DELAY);
    }

    /// Scripted responses: each call pops the next (status, Retry-After) and
    /// records the (virtual) instant of the attempt.
    struct Script {
        replies: std::sync::Mutex<std::collections::VecDeque<(u16, Option<&'static str>)>>,
        at: std::sync::Mutex<Vec<tokio::time::Instant>>,
    }

    impl Script {
        fn new(replies: &[(u16, Option<&'static str>)]) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                replies: std::sync::Mutex::new(replies.iter().copied().collect()),
                at: std::sync::Mutex::new(vec![]),
            })
        }
        fn gaps(&self) -> Vec<Duration> {
            let at = self.at.lock().unwrap();
            at.windows(2).map(|w| w[1] - w[0]).collect()
        }
    }

    async fn drive(
        p: RetryPolicy,
        mode: RetryMode,
        script: &std::sync::Arc<Script>,
    ) -> Result<Bytes> {
        let cancel = CancellationToken::new();
        send_with_retry(p, &cancel, mode, "op", || {
            let s = script.clone();
            async move {
                s.at.lock().unwrap().push(tokio::time::Instant::now());
                let (status, ra) = s
                    .replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected attempt");
                let mut b = http::Response::builder().status(status);
                if let Some(ra) = ra {
                    b = b.header("Retry-After", ra);
                }
                Ok(reqwest::Response::from(b.body("{}").unwrap()))
            }
        })
        .await
    }

    // L-426 (TST-API-050): exact attempt counts and delays per request kind.
    #[tokio::test(start_paused = true)]
    async fn attempts_and_delays_per_request_kind() {
        // Idempotent kinds (initial search, PIT, whole _bulk, meta PUTs): 429 twice then 200.
        let s = Script::new(&[(429, None), (429, None), (200, None)]);
        drive(policy(), RetryMode::Idempotent, &s).await.unwrap();
        assert_eq!(s.gaps(), vec![Duration::from_millis(100); 2]);
        // Exponential backoff: 100, 200, 400 ms; then attempts exhausted (1 + 3).
        let mut p = policy();
        p.backoff = Backoff::Exponential;
        let s = Script::new(&[(503, None), (502, None), (500, None), (504, None)]);
        let e = drive(p, RetryMode::Idempotent, &s).await.unwrap_err();
        assert!(e.to_string().contains("HTTP 504"), "{e}");
        assert_eq!(
            s.gaps(),
            [100, 200, 400].map(Duration::from_millis).to_vec()
        );
        // Scroll continuation: 429 is retried ...
        let s = Script::new(&[(429, None), (200, None)]);
        drive(policy(), RetryMode::RejectionOnly, &s).await.unwrap();
        assert_eq!(s.gaps().len(), 1);
        // ... 5xx is not (one attempt only).
        let s = Script::new(&[(503, None)]);
        assert!(drive(policy(), RetryMode::RejectionOnly, &s).await.is_err());
        assert_eq!(s.at.lock().unwrap().len(), 1);
        // --retryAttempts 0: exactly one attempt.
        let mut p = policy();
        p.attempts = 0;
        let s = Script::new(&[(429, None)]);
        assert!(drive(p, RetryMode::Idempotent, &s).await.is_err());
        assert_eq!(s.at.lock().unwrap().len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn retry_after_is_a_capped_minimum() {
        let s = Script::new(&[
            (429, Some("7")),
            (503, Some("600")),
            (429, Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            (200, None),
        ]);
        drive(policy(), RetryMode::Idempotent, &s).await.unwrap();
        assert_eq!(
            s.gaps(),
            vec![
                Duration::from_secs(7),
                MAX_RETRY_DELAY,
                Duration::from_millis(100)
            ]
        );
        // Retry-After below the configured delay does not shorten it.
        let mut p = policy();
        p.base_delay = Duration::from_secs(5);
        let s = Script::new(&[(429, Some("1")), (200, None)]);
        drive(p, RetryMode::RejectionOnly, &s).await.unwrap();
        assert_eq!(s.gaps(), vec![Duration::from_secs(5)]);
    }

    // L-430 (TST-API-054): 401/403 name the endpoint and the privilege; no secret.
    #[test]
    fn auth_errors_name_endpoint_and_privilege() {
        let u =
            url::Url::parse("https://bob:hunter2@es.local:9200/logs/_search?scroll=5m").unwrap();
        assert_eq!(public_endpoint(&u), "https://es.local:9200/logs/_search");
        let e = |status, ep: &str| {
            EsHttpError {
                prefix: String::new(),
                operation: "op".into(),
                status,
                body: "{}".into(),
                endpoint: ep.into(),
            }
            .to_string()
        };
        assert!(e(401, "https://h:9200/").contains("authentication failed at https://h:9200/"));
        for (path, want) in [
            ("/", "'monitor' cluster"),
            ("/idx/_search", "'read' index"),
            ("/_search/scroll", "'read' index"),
            ("/idx/_pit", "'read' index"),
            ("/_bulk", "'write'"),
            ("/_ilm/policy/p", "'manage_ilm'"),
            ("/_index_template/t", "'manage_index_templates'"),
            ("/_component_template/c", "'manage_index_templates'"),
            ("/_template/t", "'manage_index_templates'"),
            ("/_plugins/_ism/policies/p", "ISM permissions"),
            ("/_cluster/state/metadata", "'monitor' cluster"),
            ("/_scripts/s", "'manage' cluster"),
            ("/idx/_mapping", "'manage' index"),
            ("/idx", "'create_index'"),
        ] {
            let m = e(403, &format!("http://h:9200{path}"));
            assert!(m.contains(want) && m.contains(path), "{path}: {m}");
        }
        assert!(!e(500, "http://h/").contains("privilege"));
    }

    // L-432 (TST-API-056): server type and reason first, body capped at 2 KB.
    #[test]
    fn server_errors_pass_type_and_reason_and_cap_the_body() {
        let e = |body: String| {
            EsHttpError {
                prefix: String::new(),
                operation: "op".into(),
                status: 400,
                body,
                endpoint: String::new(),
            }
            .to_string()
        };
        let m = e(r#"{"error":{"root_cause":[],"type":"mapper_parsing_exception","reason":"failed to parse field [n] of type [long]","caused_by":{"type":"illegal_argument_exception","reason":"For input string: \"abc\""}},"status":400}"#.into());
        assert!(m.contains("HTTP 400: mapper_parsing_exception: failed to parse field [n] of type [long] (caused by illegal_argument_exception: For input string: \"abc\"); body: {"), "{m}");
        let huge = format!(
            r#"{{"error":{{"type":"illegal_argument_exception","reason":"{}"}}}}"#,
            "é".repeat(5000)
        );
        let m = e(huge);
        assert!(
            m.starts_with("Elasticsearch op failed with HTTP 400: illegal_argument_exception: "),
            "{m}"
        );
        assert!(m.len() <= 2 * MAX_ERROR_BODY + 200, "{}", m.len());
        assert!(m.contains("more bytes)"));
        assert_eq!(
            e("plain text".into()),
            "Elasticsearch op failed with HTTP 400: plain text"
        );
    }

    /// L-804a: a final HTTP 429 names the knobs that help (storm runs exited with the raw
    /// es_rejected_execution_exception only).
    #[test]
    fn final_429_suggests_retry_and_load_knobs() {
        let m = EsHttpError {
            prefix: String::new(),
            operation: "bulk".into(),
            status: 429,
            body: r#"{"error":{"type":"es_rejected_execution_exception","reason":"rejected execution"},"status":429}"#.into(),
            endpoint: String::new(),
        }
        .to_string();
        assert!(
            m.starts_with(
                "Elasticsearch bulk failed with HTTP 429: es_rejected_execution_exception"
            ),
            "{m}"
        );
        for knob in [
            "--retryAttempts",
            "--retryDelay",
            "--concurrency",
            "--limit",
        ] {
            assert!(m.contains(knob), "{knob}: {m}");
        }
    }

    #[test]
    fn is_cancelled_detects_wrapped_sentinel() {
        let e = anyhow::Error::new(DumpCancelled).context("outer");
        assert!(is_cancelled(&e));
        assert!(!is_cancelled(&anyhow!("other")));
    }

    #[test]
    fn http_errors_keep_status_and_type() {
        let err = validate_response_bytes(
            400,
            Bytes::from_static(br#"{"error":{"type":"resource_already_exists_exception"}}"#),
            "",
            "index create",
        )
        .unwrap_err();
        let http = err.downcast_ref::<EsHttpError>().unwrap();
        assert_eq!(http.status, 400);
        assert!(http.error_type_is("resource_already_exists_exception"));
        assert!(validate_response_bytes(200, Bytes::new(), "", "x").is_ok());
    }
}
