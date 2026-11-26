//! Thin Elasticsearch/OpenSearch HTTP client on top of reqwest.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use base64::prelude::*;
use bytes::Bytes;
use http::header::{
    ACCEPT, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue,
    USER_AGENT,
};
use reqwest::Method;
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;
use url::Url;

use super::gate::RequestGate;
use super::info::{ClusterInfo, Detection, NODES_FILTER};
use super::retry::{RetryMode, RetryPolicy, send_with_retry};
use super::tls::{TlsOptions, build_client_config};

/// Request body variants. Bytes are cheap to clone, so retries resend them.
#[derive(Debug, Clone)]
pub enum Body {
    Empty,
    Json(Bytes),
    Ndjson(Bytes),
    /// Gzip-compressed NDJSON (`--esCompress` on bulk writes).
    NdjsonGzip(Bytes),
}

impl Body {
    pub fn json(value: &impl serde::Serialize) -> Self {
        Body::Json(Bytes::from(
            serde_json::to_vec(value).expect("serializable body"),
        ))
    }

    pub fn sonic(value: &sonic_rs::Value) -> Self {
        Body::Json(Bytes::from(
            sonic_rs::to_vec(value).expect("serializable body"),
        ))
    }
}

/// Everything needed to build a client for one endpoint.
#[derive(Clone)]
pub struct ClientOptions {
    /// Base URL ending in `/` (may include a proxy base path).
    pub base: Url,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Extra request headers (`--headers`, `--{input,output}-headers`), already
    /// merged in precedence order.
    pub headers: Vec<(String, String)>,
    /// Extra query parameters (`--{input,output}-params` or `--params`).
    pub params: Vec<(String, String)>,
    pub tls: TlsOptions,
    pub timeout: Option<Duration>,
    /// Longest silence while waiting for or reading a response (`--idleTimeout`).
    pub idle_timeout: Option<Duration>,
    /// Ask for compressed responses and gzip bulk request bodies.
    pub compress: bool,
    pub max_sockets: Option<usize>,
    pub debug: bool,
    /// `--noProxy`: ignore HTTP(S)_PROXY / ALL_PROXY from the environment.
    pub no_proxy: bool,
}

/// Manual `Debug` that never prints the password, header values or query
/// parameter values (F-08).
impl std::fmt::Debug for ClientOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names =
            |v: &Vec<(String, String)>| v.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        f.debug_struct("ClientOptions")
            .field("base", &crate::endpoint::redacted_url(&self.base))
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("header_names", &names(&self.headers))
            .field("param_names", &names(&self.params))
            .field("tls", &self.tls)
            .field("timeout", &self.timeout)
            .field("idle_timeout", &self.idle_timeout)
            .field("compress", &self.compress)
            .field("max_sockets", &self.max_sockets)
            .field("debug", &self.debug)
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

impl ClientOptions {
    pub fn new(base: Url) -> Self {
        Self {
            base,
            username: None,
            password: None,
            headers: vec![],
            params: vec![],
            tls: TlsOptions::default(),
            timeout: None,
            idle_timeout: None,
            compress: false,
            max_sockets: None,
            debug: false,
            no_proxy: false,
        }
    }
}

struct Inner {
    http: reqwest::Client,
    base: Url,
    params: Vec<(String, String)>,
    compress: bool,
    gate: RequestGate,
    info: OnceCell<ClusterInfo>,
    debug: bool,
    /// basic auth or an `Authorization`/`x-api-key` header is sent (L-656)
    credentials: bool,
}

/// Query parameters whose values are safe to log; every other value (for
/// example an `api_key` passed with `--params`) is masked (F-05).
const LOGGABLE_PARAMS: &[&str] = &[
    "scroll",
    "keep_alive",
    "size",
    "from",
    "refresh",
    "wait_for_status",
    "timeout",
    "ignore_unavailable",
    "allow_no_indices",
    "expand_wildcards",
    "pretty",
    "format",
    "include_type_name",
    "master_timeout",
    "cluster_manager_timeout",
    "seq_no_primary_term",
    "version",
    "slices",
    "flat_settings",
    "include_defaults",
    "local",
];

/// TST-API-041: deprecation `Warning` headers (RFC 7234 `299 <agent> "<text>" ...`) are not
/// errors; each distinct text is logged once per run.
fn log_server_warnings(headers: &reqwest::header::HeaderMap) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    for v in headers.get_all(reqwest::header::WARNING) {
        let Ok(raw) = v.to_str() else { continue };
        let text = warning_text(raw);
        let seen = SEEN.get_or_init(|| Mutex::new(HashSet::new()));
        if seen
            .lock()
            .map(|mut s| s.insert(text.clone()))
            .unwrap_or(false)
        {
            log::warn!("server warning: {text}");
        }
    }
}

/// The quoted text of a `Warning` header value, or the whole value.
fn warning_text(raw: &str) -> String {
    let mut parts = raw.splitn(2, '"');
    let _code_agent = parts.next();
    match parts.next() {
        Some(rest) => {
            let mut out = String::new();
            let mut esc = false;
            for ch in rest.chars() {
                match (esc, ch) {
                    (false, '\\') => esc = true,
                    (false, '"') => break,
                    (_, c) => {
                        out.push(c);
                        esc = false;
                    }
                }
            }
            out
        }
        None => raw.to_string(),
    }
}

/// A request URL for the debug log: password and non-allow-listed query
/// values masked.
pub fn redacted_request_url(url: &Url) -> String {
    let mut out = url.clone();
    if url.password().is_some() {
        let _ = out.set_password(Some("***"));
    }
    if url.query().is_some() {
        let pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| {
                let v = if LOGGABLE_PARAMS.contains(&k.as_ref()) {
                    v.into_owned()
                } else {
                    "***".to_string()
                };
                (k.into_owned(), v)
            })
            .collect();
        out.query_pairs_mut().clear().extend_pairs(pairs);
    }
    out.to_string()
}

/// The proxy reqwest would use for `base` from the environment, if any:
/// `(variable, proxy URL)`. `NO_PROXY` entries (`*`, exact host or
/// `.suffix`/`suffix`) disable it.
fn env_proxy_for(base: &Url) -> Option<(String, String)> {
    let get = |k: &str| {
        std::env::var(k)
            .ok()
            .or_else(|| std::env::var(k.to_ascii_lowercase()).ok())
            .filter(|v| !v.trim().is_empty())
    };
    let host = base.host_str().unwrap_or("").to_ascii_lowercase();
    if let Some(np) = get("NO_PROXY") {
        for entry in np.split(',').map(|e| e.trim().to_ascii_lowercase()) {
            let e = entry.trim_start_matches('.');
            if entry == "*" || (!e.is_empty() && (host == e || host.ends_with(&format!(".{e}")))) {
                return None;
            }
        }
    }
    let scheme_var = if base.scheme() == "https" {
        "HTTPS_PROXY"
    } else {
        "HTTP_PROXY"
    };
    [scheme_var, "ALL_PROXY"]
        .into_iter()
        .find_map(|k| get(k).map(|v| (k.to_string(), v)))
}

/// Error from [`EsClient::send`]: a transport error, or a request that was
/// refused before anything was sent because its path was unsafe.
#[derive(Debug)]
pub enum SendError {
    Http(reqwest::Error),
    InvalidPath(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Http(e) => e.fmt(f),
            SendError::InvalidPath(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for SendError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SendError::Http(e) => Some(e),
            SendError::InvalidPath(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct EsClient(Arc<Inner>);

impl std::fmt::Debug for EsClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EsClient")
            .field("base", &self.0.base.as_str())
            .finish()
    }
}

impl EsClient {
    pub fn new(opts: &ClientOptions, gate: RequestGate) -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("elasticdump"));
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        match (opts.username.as_deref(), opts.password.as_deref()) {
            (Some(user), pass) => {
                if pass.is_none() {
                    log::warn!(
                        "No password provided for user '{user}'; sending basic auth with an empty password"
                    );
                }
                let token = BASE64_STANDARD.encode(format!("{user}:{}", pass.unwrap_or("")));
                let mut value = HeaderValue::from_str(&format!("Basic {token}"))
                    .context("Invalid basic-auth credentials")?;
                value.set_sensitive(true);
                headers.insert(AUTHORIZATION, value);
            }
            (None, Some(_)) => {
                return Err(anyhow!("A password was provided without a username"));
            }
            (None, None) => {}
        }

        for (name, value) in &opts.headers {
            let name = HeaderName::from_bytes(name.as_bytes())
                .with_context(|| format!("Invalid header name '{name}'"))?;
            let mut value = HeaderValue::from_str(value)
                .with_context(|| format!("Invalid value for header '{name}'"))?;
            if matches!(name.as_str(), "authorization" | "cookie" | "x-api-key") {
                value.set_sensitive(true);
            }
            headers.insert(name, value);
        }

        let credentials = headers.contains_key(AUTHORIZATION) || headers.contains_key("x-api-key");
        if opts.tls.insecure {
            log::warn!(
                "TLS certificate verification is DISABLED for {}",
                crate::endpoint::redacted_url(&opts.base)
            );
        }
        let tls = build_client_config(&opts.tls)?;

        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .tls_backend_preconfigured(tls)
            .gzip(opts.compress)
            // never follow redirects: a redirect could carry credentials or data to another
            // host, and API calls must reach the cluster that was named (TST-BRK-NET-012)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(timeout) = opts.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(idle) = opts.idle_timeout {
            // a silent server during connect or the TLS handshake is covered
            // too (read_timeout only starts with the response; found in L-453)
            builder = builder.read_timeout(idle).connect_timeout(idle);
        }
        if let Some(n) = opts.max_sockets {
            builder = builder.pool_max_idle_per_host(n);
        }
        if opts.no_proxy {
            builder = builder.no_proxy();
        } else if let Some((var, proxy)) = env_proxy_for(&opts.base) {
            // reqwest honours the proxy environment like Node's `request`;
            // make the extra hop visible (F-07).
            log::warn!(
                "Requests to {} go through proxy {} (from {var}); use --noProxy to connect directly",
                crate::endpoint::redacted_url(&opts.base),
                crate::endpoint::redacted_endpoint(&proxy)
            );
        }
        let http = builder.build().context("Failed to build HTTP client")?;

        let mut base = opts.base.clone();
        if !base.path().ends_with('/') {
            let p = format!("{}/", base.path());
            base.set_path(&p);
        }

        Ok(Self(Arc::new(Inner {
            http,
            base,
            params: opts.params.clone(),
            compress: opts.compress,
            gate,
            info: OnceCell::new(),
            debug: opts.debug,
            credentials,
        })))
    }

    pub fn compress_requests(&self) -> bool {
        self.0.compress
    }

    /// Build the URL for path `segments` below the base (each segment is
    /// percent-encoded individually) plus query pairs.
    ///
    /// A `.`, `..` or empty segment is an error: the `url` crate would drop
    /// it silently, so `PUT /{..}/_settings` would become `PUT /_settings`
    /// (INJ-F2, TST-INJ-007).
    pub fn url(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
        with_params: bool,
    ) -> std::result::Result<Url, SendError> {
        // Control characters: the URL parser drops tab, CR and LF from paths, so
        // "lo\ngs" would silently address "logs" (L-512 fuzz_url_segments finding).
        if let Some(bad) = segments
            .iter()
            .find(|s| s.is_empty() || **s == "." || **s == ".." || s.chars().any(char::is_control))
        {
            return Err(SendError::InvalidPath(format!(
                "refusing to send a request with the path segment {bad:?} in /{}",
                segments.join("/")
            )));
        }
        let mut url = self.0.base.clone();
        {
            let mut segs = url.path_segments_mut().expect("http(s) base URL");
            segs.pop_if_empty();
            for s in segments {
                segs.push(s);
            }
        }
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in query {
                pairs.append_pair(k, v);
            }
            if with_params {
                for (k, v) in &self.0.params {
                    pairs.append_pair(k, v);
                }
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        Ok(url)
    }

    /// Send one request. `segments` are path segments below the base URL;
    /// `with_params` appends the user's `--params` for this direction.
    pub async fn send(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, &str)],
        with_params: bool,
        body: Body,
    ) -> std::result::Result<reqwest::Response, SendError> {
        let url = self.url(segments, query, with_params)?;
        if self.0.debug {
            log::debug!("{} {}", method, redacted_request_url(&url));
        }
        let mut rb = self.0.http.request(method, url);
        rb = match body {
            Body::Empty => rb,
            Body::Json(b) => rb.header(CONTENT_TYPE, "application/json").body(b),
            Body::Ndjson(b) => rb.header(CONTENT_TYPE, "application/x-ndjson").body(b),
            Body::NdjsonGzip(b) => rb
                .header(CONTENT_TYPE, "application/x-ndjson")
                .header(CONTENT_ENCODING, "gzip")
                .body(b),
        };
        let _permit = self.0.gate.acquire().await;
        let resp = rb.send().await.map_err(SendError::Http)?;
        log_server_warnings(resp.headers());
        Ok(resp)
    }

    /// Cluster flavor and version (`GET /`), fetched once and cached.
    pub async fn info(
        &self,
        retry: RetryPolicy,
        cancel: &CancellationToken,
    ) -> Result<&ClusterInfo> {
        self.0
            .info
            .get_or_try_init(|| async {
                let body = send_with_retry(
                    retry,
                    cancel,
                    RetryMode::Idempotent,
                    "version check",
                    move || self.send(Method::GET, &[], &[], false, Body::Empty),
                )
                .await
                .with_context(|| {
                    format!(
                        "Failed to contact {}",
                        crate::endpoint::redacted_url(&self.0.base)
                    )
                })?;
                let detect_context = || {
                    format!(
                        "Failed to detect the engine at {}",
                        crate::endpoint::redacted_url(&self.0.base)
                    )
                };
                let info = match ClusterInfo::detect(&body).with_context(detect_context)? {
                    Detection::Known(info) => info,
                    // L-654: `GET /` is ambiguous (OpenSearch with
                    // `compatibility.override_main_response_version`); the nodes tell.
                    Detection::Probe(probe) => {
                        let nodes = send_with_retry(
                            retry,
                            cancel,
                            RetryMode::Idempotent,
                            "version check (GET _nodes)",
                            move || {
                                self.send(
                                    Method::GET,
                                    &["_nodes"],
                                    &[("filter_path", NODES_FILTER)],
                                    false,
                                    Body::Empty,
                                )
                            },
                        )
                        .await
                        .map_err(|e| format!("{e:#}"));
                        probe
                            .resolve(Some(nodes.as_deref().map_err(String::clone)))
                            .with_context(detect_context)?
                    }
                };
                log::debug!(
                    "Detected {} {} at {}",
                    info.flavor_name(),
                    info.version,
                    crate::endpoint::redacted_url(&self.0.base)
                );
                if info.oss && self.0.credentials {
                    // L-656 (BRK-XV-005): an OSS build ignores credentials without an error
                    log::warn!(
                        "{} is Elasticsearch {} (OSS build), which has no X-Pack security: it does not check the credentials given for it (only a proxy in front of it could)",
                        crate::endpoint::redacted_url(&self.0.base),
                        info.version
                    );
                }
                Ok(info)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(base: &str, params: Vec<(String, String)>) -> EsClient {
        let mut o = ClientOptions::new(Url::parse(base).unwrap());
        o.params = params;
        EsClient::new(&o, RequestGate::default()).unwrap()
    }

    #[test]
    fn warning_header_text_is_extracted() {
        assert_eq!(
            super::warning_text(
                r#"299 Elasticsearch-8.19.23-abc "Legacy index templates are deprecated in favor of composable templates." "Thu, 30 Oct 2026 06:00:00 GMT""#
            ),
            "Legacy index templates are deprecated in favor of composable templates."
        );
        assert_eq!(
            super::warning_text(r#"299 OpenSearch-2.19 "a \"quoted\" b""#),
            r#"a "quoted" b"#
        );
        assert_eq!(super::warning_text("plain"), "plain");
    }

    #[test]
    fn url_joins_segments_under_base_path_and_encodes() {
        let c = client("https://h:9200/proxy", vec![]);
        assert_eq!(
            c.url(&["my index", "_search"], &[("scroll", "1m")], false)
                .unwrap()
                .as_str(),
            "https://h:9200/proxy/my%20index/_search?scroll=1m"
        );
        let c = client(
            "http://h:9200/",
            vec![("preference".into(), "_local".into())],
        );
        assert_eq!(c.url(&[], &[], false).unwrap().as_str(), "http://h:9200/");
        assert_eq!(
            c.url(&["a,b*", "_bulk"], &[], true).unwrap().as_str(),
            "http://h:9200/a,b*/_bulk?preference=_local"
        );
    }

    /// TST-INJ-007: segments never vanish; unsafe ones are an error.
    #[test]
    fn url_segments_never_vanish() {
        let c = client("http://h:9200/base", vec![]);
        // L-512: the URL parser strips tab/CR/LF, which would merge "lo\ngs" into "logs"
        for bad in [".", "..", "", "lo\ngs", "a\tb", "x\r", "\u{7f}"] {
            assert!(c.url(&[bad, "_settings"], &[], false).is_err(), "{bad:?}");
            assert!(c.url(&["i", bad], &[], false).is_err(), "{bad:?}");
        }
        let samples = [
            "a/b",
            "a?b",
            "a#b",
            "a%b",
            "a b",
            "\u{e9}t\u{e9}",
            "...",
            ".a",
            "a.",
            "%2e",
            "%2E%2E",
            "+",
            "\u{1F600}",
        ];
        for s in samples {
            let u = c.url(&[s, "_doc", s], &[], false).unwrap();
            let segs: Vec<&str> = u.path_segments().unwrap().collect();
            assert_eq!(segs.len(), 4, "{s:?} -> {u}");
            assert_eq!(segs[0], "base");
            let decoded = percent_encoding::percent_decode_str(segs[1])
                .decode_utf8()
                .unwrap();
            assert_eq!(decoded, s, "{s:?} -> {u}");
            for ch in ['/', '?', '#', ' '] {
                assert!(!segs[1].contains(ch), "{s:?} -> {u}");
            }
        }
    }

    /// TST-INJ-012: query values with & = # % + and Unicode are encoded so
    /// the server decodes them unchanged (golden URLs).
    #[test]
    fn query_values_are_encoded_exactly() {
        let c = client(
            "http://h:9200/",
            vec![("q".into(), "a&b=c#d%e+f \u{e9}".into())],
        );
        let u = c
            .url(&["i", "_search"], &[("scroll_id", "AbC==/+x")], true)
            .unwrap();
        assert_eq!(
            u.as_str(),
            "http://h:9200/i/_search?scroll_id=AbC%3D%3D%2F%2Bx&q=a%26b%3Dc%23d%25e%2Bf+%C3%A9"
        );
        let pairs: Vec<(String, String)> = u.query_pairs().into_owned().collect();
        assert_eq!(
            pairs,
            vec![
                ("scroll_id".to_string(), "AbC==/+x".to_string()),
                ("q".to_string(), "a&b=c#d%e+f \u{e9}".to_string())
            ]
        );
    }

    /// F-05: --debug never logs secret query values or passwords.
    #[test]
    fn debug_url_masks_params_and_password() {
        let u =
            Url::parse("http://u:pw@h:9200/i/_search?scroll=1m&api_key=SECRET&size=10").unwrap();
        let r = redacted_request_url(&u);
        assert_eq!(
            r,
            "http://u:***@h:9200/i/_search?scroll=1m&api_key=***&size=10"
        );
        assert!(!r.contains("SECRET") && !r.contains(":pw@"));
    }

    /// F-08: Debug output never contains secrets.
    #[test]
    fn client_options_debug_is_redacted() {
        let mut o = ClientOptions::new(Url::parse("http://h:9200/").unwrap());
        o.password = Some("PW-SECRET".into());
        o.headers = vec![("Authorization".into(), "ApiKey HDR-SECRET".into())];
        o.params = vec![("api_key".into(), "Q-SECRET".into())];
        o.tls.pass = Some("TLS-SECRET".into());
        let d = format!("{o:?}");
        for s in ["PW-SECRET", "HDR-SECRET", "Q-SECRET", "TLS-SECRET"] {
            assert!(!d.contains(s), "{d}");
        }
        assert!(d.contains("Authorization"), "{d}");
    }

    #[test]
    fn password_without_user_is_rejected() {
        let mut o = ClientOptions::new(Url::parse("http://h:9200/").unwrap());
        o.password = Some("x".into());
        let err = EsClient::new(&o, RequestGate::default()).unwrap_err();
        assert!(err.to_string().contains("without a username"));
    }

    #[test]
    fn invalid_header_is_rejected() {
        let mut o = ClientOptions::new(Url::parse("http://h:9200/").unwrap());
        o.headers = vec![("bad header".into(), "v".into())];
        assert!(EsClient::new(&o, RequestGate::default()).is_err());
    }
}
