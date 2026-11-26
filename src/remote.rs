//! L-809a: the OpenDAL storage path (owner request, 2026-11-07). A second, separate way to
//! read and write dump files: `s3://`, `b2://`, `sftp://` and `fs://` / `file://` URLs go
//! through [OpenDAL](https://opendal.apache.org) operators; plain paths keep using the native
//! `std::fs` code in `output.rs` and `pipeline/file_source.rs`, which this does not replace.
//!
//! Settings (endpoint, region, credentials, SSE-C key, ...) are looked up per key, first
//! match wins:
//!
//! 1. a URL query parameter, for non-secret settings only
//!    (`s3://bucket/key?endpoint=http://127.0.0.1:9000&region=eu-west-1`); a secret or a
//!    password in the URL is refused;
//! 2. the INI file of `--inputStorageConfig` / `--outputStorageConfig` (or
//!    `SEARCHDUMP_INPUT_STORAGE_CONFIG` / `SEARCHDUMP_OUTPUT_STORAGE_CONFIG`), the storage
//!    counterpart of `--httpAuthFile`: top-level `key = value` lines, optionally overridden by a
//!    `[<scheme>]` and then a `[<scheme>.<bucket-or-host>]` section;
//! 3. the environment: `SEARCHDUMP_<SCHEME>_<SETTING>` (`SEARCHDUMP_S3_SSE_C_KEY_FILE`), then
//!    the standard names (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`,
//!    `AWS_ENDPOINT_URL`, `AWS_REGION`, `B2_APPLICATION_KEY_ID`, `B2_APPLICATION_KEY`).
//!
//! Secret values (keys, tokens, the SSE-C key) are registered with [`crate::secrets`] so they
//! are scrubbed from every log line and error, and are never shown in [`RemoteSpec`]'s
//! `Debug` or display form.
//!
//! The URL parsing, settings lookup and redaction in this file are always compiled (and unit
//! tested); building an OpenDAL operator needs the `opendal` cargo feature (default on).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use anyhow::{Result, anyhow, bail};
use url::Url;

/// Storage backends reached through OpenDAL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    S3,
    B2,
    Sftp,
    Fs,
}

impl Scheme {
    pub fn name(self) -> &'static str {
        match self {
            Scheme::S3 => "s3",
            Scheme::B2 => "b2",
            Scheme::Sftp => "sftp",
            Scheme::Fs => "fs",
        }
    }

    /// The scheme of `raw` when it is one of ours (`file://` is an alias of `fs://`).
    pub fn of(raw: &str) -> Option<Scheme> {
        let lower = raw.get(..raw.find("://")?)?.to_ascii_lowercase();
        match lower.as_str() {
            "s3" => Some(Scheme::S3),
            "b2" => Some(Scheme::B2),
            "sftp" => Some(Scheme::Sftp),
            "fs" | "file" => Some(Scheme::Fs),
            _ => None,
        }
    }

    /// Setting keys this backend understands; anything else in a URL query is refused.
    fn keys(self) -> &'static [&'static str] {
        match self {
            Scheme::S3 => &[
                "endpoint",
                "region",
                "access_key_id",
                "secret_access_key",
                "session_token",
                "virtual_host_style",
                "allow_anonymous",
                "disable_config_load",
                "sse_c_algorithm",
                "sse_c_key",
                "sse_c_key_file",
                "storage_class",
                "ca_file",
                "insecure",
                "chunk_mb",
                "concurrent",
            ],
            Scheme::B2 => &[
                "application_key_id",
                "application_key",
                "bucket_id",
                "chunk_mb",
                "concurrent",
            ],
            Scheme::Sftp => &["key", "known_hosts", "user"],
            Scheme::Fs => &[],
        }
    }
}

/// Keys whose values are secrets: redacted, registered with the scrubber, and warned about
/// when given in a URL (where `ps` and shell history can see them).
pub fn is_secret_key(key: &str) -> bool {
    matches!(
        key,
        "secret_access_key" | "session_token" | "sse_c_key" | "application_key"
    )
}

/// Short INI names accepted in a storage config file, as `--httpAuthFile` has `user=` and
/// `password=`: (alias, scheme, setting).
const INI_ALIASES: &[(&str, Scheme, &str)] = &[
    ("key", Scheme::S3, "access_key_id"),
    ("secret", Scheme::S3, "secret_access_key"),
    ("token", Scheme::S3, "session_token"),
    ("key", Scheme::B2, "application_key_id"),
    ("secret", Scheme::B2, "application_key"),
    ("key_file", Scheme::Sftp, "key"),
];

/// Standard environment names read after `SEARCHDUMP_<SCHEME>_<SETTING>`.
fn standard_env(scheme: Scheme, key: &str) -> &'static [&'static str] {
    match (scheme, key) {
        (Scheme::S3, "access_key_id") => &["AWS_ACCESS_KEY_ID"],
        (Scheme::S3, "secret_access_key") => &["AWS_SECRET_ACCESS_KEY"],
        (Scheme::S3, "session_token") => &["AWS_SESSION_TOKEN"],
        (Scheme::S3, "endpoint") => &["AWS_ENDPOINT_URL_S3", "AWS_ENDPOINT_URL"],
        (Scheme::S3, "region") => &["AWS_REGION", "AWS_DEFAULT_REGION"],
        (Scheme::B2, "application_key_id") => &["B2_APPLICATION_KEY_ID"],
        (Scheme::B2, "application_key") => &["B2_APPLICATION_KEY"],
        _ => &[],
    }
}

static CONFIG_FILES: OnceLock<[Option<PathBuf>; 2]> = OnceLock::new();

fn env_config(dir: crate::endpoint::Direction) -> Option<PathBuf> {
    crate::envnames::var_os(&format!("{}_STORAGE_CONFIG", dir.env_prefix()))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `--inputStorageConfig` / `--outputStorageConfig` (else their `SEARCHDUMP_*` variables).
/// Set once at start-up.
pub fn set_config_files(input: Option<PathBuf>, output: Option<PathBuf>) {
    use crate::endpoint::Direction;
    let _ = CONFIG_FILES.set([
        input.or_else(|| env_config(Direction::Input)),
        output.or_else(|| env_config(Direction::Output)),
    ]);
}

/// The storage config file for one side of the run.
pub fn config_file(dir: crate::endpoint::Direction) -> Option<PathBuf> {
    use crate::endpoint::Direction;
    let files =
        CONFIG_FILES.get_or_init(|| [env_config(Direction::Input), env_config(Direction::Output)]);
    files[match dir {
        Direction::Input => 0,
        Direction::Output => 1,
    }]
    .clone()
}

/// One storage location: a backend, its bucket or host, and an object key (or, for
/// searchdump-multi's `--output`, a key prefix).
pub struct RemoteSpec {
    pub scheme: Scheme,
    /// S3/B2 bucket, SFTP `host[:port]`; empty for `fs`.
    pub authority: String,
    /// SFTP user from `sftp://user@host`.
    pub user: Option<String>,
    /// Object key below the operator root, without a leading `/`.
    pub key: String,
    /// Settings resolved from the URL, env and config file.
    settings: BTreeMap<String, String>,
    /// URL for messages, with every secret masked.
    display: String,
    #[cfg(feature = "opendal")]
    op: OnceLock<opendal::Operator>,
}

impl Clone for RemoteSpec {
    fn clone(&self) -> Self {
        Self {
            scheme: self.scheme,
            authority: self.authority.clone(),
            user: self.user.clone(),
            key: self.key.clone(),
            settings: self.settings.clone(),
            display: self.display.clone(),
            #[cfg(feature = "opendal")]
            op: self.op.clone(),
        }
    }
}

impl std::fmt::Debug for RemoteSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let shown: BTreeMap<&str, &str> = self
            .settings
            .iter()
            .map(|(k, v)| {
                let v = if is_secret_key(k) { "***" } else { v.as_str() };
                (k.as_str(), v)
            })
            .collect();
        f.debug_struct("RemoteSpec")
            .field("url", &self.display)
            .field("settings", &shown)
            .finish()
    }
}

impl PartialEq for RemoteSpec {
    fn eq(&self, o: &Self) -> bool {
        self.scheme == o.scheme
            && self.authority == o.authority
            && self.user == o.user
            && self.key == o.key
            && self.settings == o.settings
    }
}
impl Eq for RemoteSpec {}

impl std::fmt::Display for RemoteSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.display)
    }
}

/// Mask secret query parameters and any `user:password@` in a storage URL for messages.
pub fn redact(raw: &str) -> String {
    let (head, query) = match raw.split_once('?') {
        Some((h, q)) => (h, Some(q)),
        None => (raw, None),
    };
    let mut out = crate::endpoint::mask_userinfo(head);
    if let Some(q) = query {
        let parts: Vec<String> = q
            .split('&')
            .map(|kv| match kv.split_once('=') {
                Some((k, _)) if is_secret_key(&k.to_ascii_lowercase()) => format!("{k}=***"),
                _ => kv.to_string(),
            })
            .collect();
        out.push('?');
        out.push_str(&parts.join("&"));
    }
    out
}

fn env_name(scheme: Scheme, key: &str) -> String {
    format!(
        "SEARCHDUMP_{}_{}",
        scheme.name().to_ascii_uppercase(),
        key.to_ascii_uppercase()
    )
}

/// Parse a storage URL. `Ok(None)` when `raw` is not one of our schemes. `config` is the
/// storage config file of that side of the run.
pub fn parse(raw: &str, config: Option<&std::path::Path>) -> Result<Option<RemoteSpec>> {
    let Some(scheme) = Scheme::of(raw) else {
        return Ok(None);
    };
    let shown = redact(raw);
    let mut query: BTreeMap<String, String> = BTreeMap::new();
    let (authority, user, key) = match scheme {
        Scheme::Fs => {
            // fs:///abs/path, fs://./rel/path, fs://rel/path; no query string.
            let rest = &raw[raw.find("://").unwrap() + 3..];
            if rest.contains('?') {
                bail!("{shown}: fs:// URLs take no ?parameters");
            }
            let p = PathBuf::from(rest);
            let abs = if p.is_absolute() {
                p
            } else {
                std::env::current_dir()
                    .map_err(|e| anyhow!("{shown}: cannot resolve a relative fs:// path: {e}"))?
                    .join(p)
            };
            let s = abs.to_string_lossy();
            let key = s.trim_start_matches('/').to_string();
            (String::new(), None, key)
        }
        _ => {
            let url = Url::parse(raw).map_err(|e| anyhow!("{shown}: not a valid URL: {e}"))?;
            if url.password().is_some() {
                bail!(
                    "{shown}: passwords in {}:// URLs are not accepted (they show in process listings and shell history); {}",
                    scheme.name(),
                    secret_hint(scheme)
                );
            }
            let host = url
                .host_str()
                .filter(|h| !h.is_empty())
                .ok_or_else(|| {
                    anyhow!(
                        "{shown}: missing {}",
                        if scheme == Scheme::Sftp {
                            "host"
                        } else {
                            "bucket"
                        }
                    )
                })?
                .to_string();
            let authority = match url.port() {
                Some(p) => format!("{host}:{p}"),
                None => host,
            };
            let user = (!url.username().is_empty()).then(|| {
                percent_encoding::percent_decode_str(url.username())
                    .decode_utf8_lossy()
                    .into_owned()
            });
            for (k, v) in url.query_pairs() {
                let k = k.to_ascii_lowercase();
                if !scheme.keys().contains(&k.as_str()) {
                    bail!(
                        "{shown}: unknown parameter '{k}' for {}:// (known: {})",
                        scheme.name(),
                        scheme.keys().join(", ")
                    );
                }
                if is_secret_key(&k) {
                    bail!(
                        "{shown}: the secret '{k}' is not accepted in the URL (it would show in process listings and shell history); {}",
                        secret_hint(scheme)
                    );
                }
                query.insert(k, v.into_owned());
            }
            let key = percent_encoding::percent_decode_str(url.path())
                .decode_utf8()
                .map_err(|e| anyhow!("{shown}: path is not UTF-8: {e}"))?
                .trim_start_matches('/')
                .to_string();
            (authority, user, key)
        }
    };
    let settings = resolve_settings(scheme, &authority, query, config)?;
    for (k, v) in &settings {
        if is_secret_key(k) {
            crate::secrets::register(v);
        }
    }
    Ok(Some(RemoteSpec {
        scheme,
        authority,
        user,
        key,
        settings,
        display: shown,
        #[cfg(feature = "opendal")]
        op: OnceLock::new(),
    }))
}

fn secret_hint(scheme: Scheme) -> &'static str {
    match scheme {
        Scheme::S3 => {
            "put it in --inputStorageConfig/--outputStorageConfig (secret=, sse_c_key_file=) or AWS_SECRET_ACCESS_KEY / SEARCHDUMP_S3_* variables"
        }
        Scheme::B2 => {
            "put it in --inputStorageConfig/--outputStorageConfig (secret=) or B2_APPLICATION_KEY"
        }
        Scheme::Sftp => {
            "use an SSH key (key_file= in --inputStorageConfig/--outputStorageConfig, ?key=/path, or SEARCHDUMP_SFTP_KEY) or ssh-agent"
        }
        Scheme::Fs => "fs:// takes no credentials",
    }
}

/// URL query, then config file (`[scheme.authority]`, `[scheme]`, top level), then env.
fn resolve_settings(
    scheme: Scheme,
    authority: &str,
    query: BTreeMap<String, String>,
    config: Option<&std::path::Path>,
) -> Result<BTreeMap<String, String>> {
    let ini =
        match config {
            Some(p) => {
                #[cfg(unix)]
                if let Ok(m) = std::fs::metadata(p) {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(w) = crate::endpoint::auth_file_mode_warning(
                        &p.display().to_string(),
                        m.permissions().mode(),
                    ) {
                        log::warn!("{w}");
                    }
                }
                Some(ini::Ini::load_from_file(p).map_err(|e| {
                    anyhow!("storage config '{}' could not be read: {e}", p.display())
                })?)
            }
            None => None,
        };
    if let Some(ini) = &ini {
        // a typo in a key name would otherwise be ignored silently
        for (sec, props) in ini.iter() {
            let ours = match sec {
                None => true,
                Some(name) => {
                    name == scheme.name() || name == format!("{}.{authority}", scheme.name())
                }
            };
            if !ours {
                continue;
            }
            for (k, _) in props.iter() {
                let known = scheme.keys().contains(&k)
                    || INI_ALIASES.iter().any(|(a, s, _)| *a == k && *s == scheme);
                if !known {
                    log::warn!(
                        "storage config '{}': unknown key '{k}' for {}:// is ignored",
                        config.map(|p| p.display().to_string()).unwrap_or_default(),
                        scheme.name()
                    );
                }
            }
        }
    }
    let ini_get = |key: &str| -> Option<String> {
        let ini = ini.as_ref()?;
        let names: Vec<&str> = std::iter::once(key)
            .chain(
                INI_ALIASES
                    .iter()
                    .filter(|(_, s, k)| *s == scheme && *k == key)
                    .map(|(a, _, _)| *a),
            )
            .collect();
        let specific = format!("{}.{authority}", scheme.name());
        for sec in [Some(specific.as_str()), Some(scheme.name()), None] {
            if let Some(props) = ini.section(sec) {
                for n in &names {
                    if let Some(v) = props.get(*n).filter(|v| !v.is_empty()) {
                        return Some(v.to_string());
                    }
                }
            }
        }
        None
    };
    let mut out = query;
    for key in scheme.keys() {
        if out.contains_key(*key) {
            continue;
        }
        if let Some(v) = ini_get(key) {
            out.insert(key.to_string(), v);
            continue;
        }
        let env = std::iter::once(env_name(scheme, key))
            .chain(standard_env(scheme, key).iter().map(|s| s.to_string()));
        for name in env {
            if let Some(v) = crate::envnames::var(&name)
                && !v.is_empty()
            {
                out.insert(key.to_string(), v);
                break;
            }
        }
    }
    Ok(out)
}

impl RemoteSpec {
    pub fn setting(&self, key: &str) -> Option<&str> {
        self.settings.get(key).map(String::as_str)
    }

    /// The same location with another key (split parts, searchdump-multi per-index files).
    pub fn with_key(&self, key: impl Into<String>) -> RemoteSpec {
        let key = key.into();
        let mut c = self.clone();
        let head = self.display.split('?').next().unwrap_or("");
        let tail = self
            .display
            .split_once('?')
            .map(|(_, q)| format!("?{q}"))
            .unwrap_or_default();
        let base = match head.find("://") {
            Some(i) => {
                let after = &head[i + 3..];
                let auth_end = if self.scheme == Scheme::Fs {
                    0
                } else {
                    after.find('/').unwrap_or(after.len())
                };
                format!("{}{}", &head[..i + 3], &after[..auth_end])
            }
            None => head.to_string(),
        };
        c.display = format!("{base}/{key}{tail}");
        c.key = key;
        c
    }

    /// Display form of the location of `key` (for messages about split parts).
    pub fn display_key(&self, key: &str) -> String {
        self.with_key(key.to_string()).display
    }

    pub fn display(&self) -> &str {
        &self.display
    }

    /// Writer chunk size: S3 multipart parts must be at least 5 MiB.
    pub fn chunk_bytes(&self) -> usize {
        let mb = self
            .setting("chunk_mb")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(16)
            .clamp(5, 5 * 1024);
        mb << 20
    }

    pub fn concurrent(&self) -> usize {
        self.setting("concurrent")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(4)
            .clamp(1, 64)
    }

    /// SSE-C key bytes (`sse_c_key` = base64 of the key, or `sse_c_key_file` = a file holding
    /// the raw 32 bytes or their base64), `None` when SSE-C is not configured.
    pub fn sse_c(&self) -> Result<Option<(String, Vec<u8>)>> {
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD;
        let key = match (self.setting("sse_c_key"), self.setting("sse_c_key_file")) {
            (Some(_), Some(_)) => bail!("{self}: give sse_c_key or sse_c_key_file, not both"),
            (Some(k), None) => b64
                .decode(k.trim())
                .map_err(|_| anyhow!("{self}: sse_c_key is not valid base64"))?,
            (None, Some(f)) => {
                let raw = std::fs::read(f).map_err(|e| {
                    anyhow!("{self}: the SSE-C key file '{f}' could not be read: {e}")
                })?;
                #[cfg(unix)]
                if let Ok(m) = std::fs::metadata(f) {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(w) =
                        crate::endpoint::auth_file_mode_warning(f, m.permissions().mode())
                    {
                        log::warn!("{w}");
                    }
                }
                if raw.len() == 32 {
                    raw
                } else {
                    let text = String::from_utf8_lossy(&raw);
                    b64.decode(text.trim()).map_err(|_| {
                        anyhow!(
                            "{self}: the SSE-C key file '{f}' must hold 32 raw bytes or their base64"
                        )
                    })?
                }
            }
            (None, None) => {
                if self.setting("sse_c_algorithm").is_some() {
                    bail!("{self}: sse_c_algorithm is set but no sse_c_key / sse_c_key_file");
                }
                return Ok(None);
            }
        };
        let alg = self.setting("sse_c_algorithm").unwrap_or("AES256");
        if alg == "AES256" && key.len() != 32 {
            bail!(
                "{self}: an AES256 SSE-C key must be 32 bytes (got {})",
                key.len()
            );
        }
        crate::secrets::register(&b64.encode(&key));
        Ok(Some((alg.to_string(), key)))
    }

    fn flag(&self, key: &str) -> Result<bool> {
        match self.setting(key).map(str::to_ascii_lowercase).as_deref() {
            None | Some("false" | "0" | "no" | "") => Ok(false),
            Some("true" | "1" | "yes") => Ok(true),
            Some(v) => bail!("{self}: {key} must be true or false, got '{v}'"),
        }
    }

    /// An OpenDAL error, with a hint for SSH connection failures (whose `ssh` stderr OpenDAL
    /// does not pass on).
    pub fn explain(&self, e: impl std::fmt::Display) -> String {
        let msg = e.to_string();
        if self.scheme == Scheme::Sftp && msg.contains("failed to connect") {
            format!(
                "{msg} (ssh could not connect or log in: check host, port and user, the key (key_file=, ssh-agent), and that the host key is in ~/.ssh/known_hosts: known_hosts=strict is the default; known_hosts=add trusts a new host once, accept trusts any. Try `ssh -p PORT user@host` by hand to see why)"
            )
        } else {
            msg
        }
    }

    /// Checks the settings without building an operator (also in builds without `opendal`).
    pub fn validate(&self) -> Result<()> {
        match self.scheme {
            Scheme::S3 => {
                self.flag("virtual_host_style")?;
                self.flag("allow_anonymous")?;
                self.flag("disable_config_load")?;
                self.flag("insecure")?;
                self.sse_c()?;
            }
            Scheme::B2 => {
                for k in ["application_key_id", "application_key", "bucket_id"] {
                    if self.setting(k).is_none() {
                        bail!(
                            "{self}: b2:// needs {k} (--inputStorageConfig/--outputStorageConfig, {} or {})",
                            env_name(Scheme::B2, k),
                            standard_env(Scheme::B2, k)
                                .first()
                                .copied()
                                .unwrap_or("?bucket_id=")
                        );
                    }
                }
            }
            Scheme::Sftp => {
                if let Some(v) = self.setting("known_hosts")
                    && !matches!(v.to_ascii_lowercase().as_str(), "strict" | "accept" | "add")
                {
                    bail!("{self}: known_hosts must be strict, accept or add");
                }
            }
            Scheme::Fs => {}
        }
        Ok(())
    }
}

/// Shared handle used by sinks and sources.
pub type Remote = Arc<RemoteSpec>;

#[cfg(feature = "opendal")]
mod op {
    use super::*;
    use opendal::Operator;
    use opendal::layers::{RetryLayer, TimeoutLayer};

    fn finish<B: opendal::Builder>(b: B, spec: &RemoteSpec) -> Result<Operator> {
        // Our rustls config: system roots (or `ca_file`), `insecure`, the ring provider; the
        // same code as the cluster side's --ca/--insecure.
        let tls = crate::http::tls::build_client_config(&crate::http::tls::TlsOptions {
            ca: spec.setting("ca_file").map(PathBuf::from),
            insecure: spec.flag("insecure")?,
            ..Default::default()
        })?;
        if spec.flag("insecure")? {
            log::warn!("{spec}: TLS certificate verification is OFF (insecure=true)");
        }
        let client = reqwest::Client::builder()
            .tls_backend_preconfigured(tls)
            .build()
            .map_err(|e| anyhow!("{spec}: HTTP client: {e}"))?;
        // Per-operator transport: no process-wide default is installed (the facade's
        // auto-registration is off), so a request can only go through this client.
        let transport = opendal::HttpTransporter::new(
            opendal_http_transport_reqwest::ReqwestTransport::new(client),
        );
        Ok(Operator::new(b)
            .map_err(|e| anyhow!("{e}"))?
            .with_context(opendal::OperationContext::new().with_http_transport(transport))
            .layer(TimeoutLayer::new().with_io_timeout(std::time::Duration::from_secs(300)))
            .layer(
                RetryLayer::new()
                    .with_max_times(5)
                    .with_jitter()
                    .with_min_delay(std::time::Duration::from_millis(500)),
            ))
    }

    impl RemoteSpec {
        /// The OpenDAL operator for this location (built once, shared by clones).
        pub fn operator(&self) -> Result<Operator> {
            if let Some(op) = self.op.get() {
                return Ok(op.clone());
            }
            crate::http::tls::install_default_provider();
            self.validate()?;
            let op = match self.scheme {
                Scheme::S3 => {
                    let mut b = opendal::services::S3::default()
                        .bucket(&self.authority)
                        .root("/");
                    if let Some(v) = self.setting("endpoint") {
                        b = b.endpoint(v);
                    }
                    // MinIO, RustFS and most S3-compatible servers ignore the region but the
                    // signature needs one.
                    b = b.region(self.setting("region").unwrap_or("us-east-1"));
                    if let Some(v) = self.setting("access_key_id") {
                        b = b.access_key_id(v);
                    }
                    if let Some(v) = self.setting("secret_access_key") {
                        b = b.secret_access_key(v);
                    }
                    if let Some(v) = self.setting("session_token") {
                        b = b.session_token(v);
                    }
                    if let Some(v) = self.setting("storage_class") {
                        b = b.default_storage_class(v);
                    }
                    if self.flag("virtual_host_style")? {
                        b = b.enable_virtual_host_style();
                    }
                    if self.flag("allow_anonymous")? {
                        b = b.skip_signature();
                    }
                    if self.flag("disable_config_load")? {
                        b = b.disable_config_load();
                    }
                    if let Some((alg, key)) = self.sse_c()? {
                        // SSE-C: the server encrypts with this key; it is sent (TLS!) with
                        // every write and read request, and never stored by the server.
                        b = b.server_side_encryption_with_customer_key(&alg, &key);
                    }
                    finish(b, self)?
                }
                Scheme::B2 => {
                    let b = opendal::services::B2::default()
                        .bucket(&self.authority)
                        .bucket_id(self.setting("bucket_id").unwrap_or_default())
                        .application_key_id(self.setting("application_key_id").unwrap_or_default())
                        .application_key(self.setting("application_key").unwrap_or_default())
                        .root("/");
                    finish(b, self)?
                }
                #[cfg(feature = "opendal-sftp")]
                Scheme::Sftp => {
                    let mut b = opendal::services::Sftp::default()
                        .endpoint(&format!("ssh://{}", self.authority))
                        .root("/");
                    if let Some(u) = self.user.as_deref().or(self.setting("user")) {
                        b = b.user(u);
                    }
                    if let Some(k) = self.setting("key") {
                        b = b.key(k);
                    }
                    b = b.known_hosts_strategy(self.setting("known_hosts").unwrap_or("strict"));
                    finish(b, self)?
                }
                #[cfg(not(feature = "opendal-sftp"))]
                Scheme::Sftp => bail!(
                    "{self}: this build has no sftp:// support (rebuild with --features opendal-sftp)"
                ),
                Scheme::Fs => {
                    let mut b = opendal::services::Fs::default().root("/");
                    // Write next to the target and rename into place, so a failed run never
                    // leaves a partial file under the final name. A directory (tree copy) is
                    // its own temp dir.
                    let abs = std::path::Path::new("/").join(&self.key);
                    let tmp = if self.key.ends_with('/') || self.key.is_empty() {
                        Some(abs.as_path())
                    } else {
                        abs.parent()
                    };
                    if let Some(dir) = tmp {
                        b = b.atomic_write_dir(&dir.to_string_lossy());
                    }
                    finish(b, self)?
                }
            };
            let _ = self.op.set(op.clone());
            Ok(op)
        }
    }
}

/// A directory side (searchdump-multi): a storage URL, or a local path read and written
/// through OpenDAL's `fs` service. The key always ends in `/` (a prefix).
pub fn parse_dir(raw: &str, config: Option<&std::path::Path>) -> Result<RemoteSpec> {
    let mut spec = match parse(raw, config)? {
        Some(s) => s,
        None => {
            if raw.is_empty() {
                bail!("empty directory");
            }
            let abs = std::path::absolute(raw)
                .map_err(|e| anyhow!("cannot resolve directory '{raw}': {e}"))?;
            parse(&format!("fs://{}", abs.display()), None)?
                .ok_or_else(|| anyhow!("cannot use directory '{raw}'"))?
        }
    };
    if !spec.key.is_empty() && !spec.key.ends_with('/') {
        let k = format!("{}/", spec.key);
        spec = spec.with_key(k);
    }
    Ok(spec)
}

/// Whether `raw` is a storage URL handled here (not a cluster, not a plain local path).
pub fn is_storage_url(raw: &str) -> bool {
    Scheme::of(raw).is_some()
}

/// What [`copy_tree`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CopyReport {
    pub files: u64,
    pub bytes: u64,
}

/// L-809a: copy every object below the directory `src` to the same relative key below `dst`,
/// byte for byte (compression, split parts, manifest and cluster files are kept as they
/// are). Without `overwrite`, any existing target object stops the copy before anything is
/// written. `parallel` objects are copied at once; each copy is checked by size.
#[cfg(feature = "opendal")]
pub async fn copy_tree(
    src: &RemoteSpec,
    dst: &RemoteSpec,
    overwrite: bool,
    parallel: usize,
) -> Result<CopyReport> {
    use futures::{StreamExt, TryStreamExt};
    let sop = src.operator()?;
    let dop = dst.operator()?;
    let entries: Vec<opendal::Entry> = sop
        .lister_with(&src.key)
        .recursive(true)
        .await
        .map_err(|e| anyhow!("listing {src} failed: {}", src.explain(e)))?
        .try_collect()
        .await
        .map_err(|e| anyhow!("listing {src} failed: {}", src.explain(e)))?;
    let mut files: Vec<String> = entries
        .into_iter()
        .filter(|e| !e.path().ends_with('/') && e.metadata().mode() != opendal::EntryMode::DIR)
        .map(|e| e.path().trim_start_matches('/').to_string())
        .filter(|p| p.starts_with(src.key.as_str()) && p.len() > src.key.len())
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("{src} holds no files to copy");
    }
    let rels: Vec<String> = files
        .iter()
        .map(|p| p[src.key.len()..].to_string())
        .collect();
    if !overwrite {
        for rel in &rels {
            let k = format!("{}{rel}", dst.key);
            if dop.exists(&k).await.map_err(|e| {
                anyhow!(
                    "checking {} failed: {}",
                    dst.display_key(&k),
                    dst.explain(e)
                )
            })? {
                bail!(
                    "{} already exists; nothing was copied (pass --overwrite to replace)",
                    dst.display_key(&k)
                );
            }
        }
    }
    let chunk = dst.chunk_bytes();
    let concurrent = dst.concurrent();
    let report = futures::stream::iter(rels.into_iter().map(|rel| {
        let (sop, dop) = (sop.clone(), dop.clone());
        let from = format!("{}{rel}", src.key);
        let to = format!("{}{rel}", dst.key);
        async move {
            let shown_from = src.display_key(&from);
            let shown_to = dst.display_key(&to);
            let size = sop
                .stat(&from)
                .await
                .map_err(|e| anyhow!("{shown_from}: {e}"))?
                .content_length();
            let mut stream = sop
                .reader_with(&from)
                .chunk(chunk)
                .await
                .map_err(|e| anyhow!("{shown_from}: {e}"))?
                .into_bytes_stream(..)
                .await
                .map_err(|e| anyhow!("{shown_from}: {e}"))?;
            let mut w = dop
                .writer_with(&to)
                .chunk(chunk)
                .concurrent(concurrent)
                .await
                .map_err(|e| anyhow!("{shown_to}: {e}"))?;
            let mut n = 0u64;
            let copied: Result<()> = async {
                while let Some(b) = stream.next().await {
                    let b = b.map_err(|e| anyhow!("reading {shown_from}: {e}"))?;
                    n += b.len() as u64;
                    w.write(b)
                        .await
                        .map_err(|e| anyhow!("writing {shown_to}: {e}"))?;
                }
                if n != size {
                    bail!("{shown_from}: read {n} bytes, expected {size}");
                }
                w.close()
                    .await
                    .map_err(|e| anyhow!("finishing {shown_to}: {e}"))?;
                Ok(())
            }
            .await;
            if let Err(e) = copied {
                let _ = w.abort().await;
                return Err(e);
            }
            let got = dop
                .stat(&to)
                .await
                .map_err(|e| anyhow!("{shown_to}: {e}"))?
                .content_length();
            if got != size {
                bail!("{shown_to} holds {got} bytes after the copy, expected {size}");
            }
            log::info!("copied {shown_from} -> {shown_to} ({size} bytes)");
            Ok::<u64, anyhow::Error>(size)
        }
    }))
    .buffer_unordered(parallel.max(1))
    .try_fold(CopyReport::default(), |mut r, size| async move {
        r.files += 1;
        r.bytes += size;
        Ok(r)
    })
    .await?;
    Ok(report)
}

/// Error for builds without the `opendal` feature.
#[cfg(not(feature = "opendal"))]
impl RemoteSpec {
    pub fn unsupported(&self) -> anyhow::Error {
        anyhow!(
            "{self}: this build has no OpenDAL storage support (rebuild with --features opendal)"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<Option<RemoteSpec>> {
        super::parse(raw, None)
    }

    fn p(raw: &str) -> RemoteSpec {
        parse(raw).unwrap().unwrap()
    }

    #[test]
    fn schemes_and_non_storage_urls() {
        assert!(parse("http://h:9200/i").unwrap().is_none());
        assert!(parse("/tmp/x.json").unwrap().is_none());
        assert!(parse("$").unwrap().is_none());
        assert_eq!(p("S3://b/k.json").scheme, Scheme::S3);
        assert_eq!(p("file:///tmp/x").scheme, Scheme::Fs);
        assert_eq!(p("fs:///tmp/x").key, "tmp/x");
    }

    #[test]
    fn s3_bucket_key_and_params() {
        let s = p("s3://bkt/dir/out.json?endpoint=http://127.0.0.1:9000&region=eu-west-1");
        assert_eq!(s.authority, "bkt");
        assert_eq!(s.key, "dir/out.json");
        assert_eq!(s.setting("endpoint"), Some("http://127.0.0.1:9000"));
        assert_eq!(s.setting("region"), Some("eu-west-1"));
        assert!(parse("s3://bkt/k?bogus=1").is_err());
        assert!(parse("s3:///k").is_err());
        assert!(
            parse("s3://u:pw@bkt/k")
                .unwrap_err()
                .to_string()
                .contains("***")
        );
    }

    #[test]
    fn sftp_user_host_port() {
        let s = p("sftp://alice@example.com:2222/home/alice/d.json?known_hosts=accept");
        assert_eq!(s.authority, "example.com:2222");
        assert_eq!(s.user.as_deref(), Some("alice"));
        assert_eq!(s.key, "home/alice/d.json");
        s.validate().unwrap();
        let bad = p("sftp://h/x?known_hosts=maybe");
        assert!(bad.validate().is_err());
    }

    #[test]
    fn secrets_in_urls_are_refused_and_redacted() {
        let raw = "s3://b/k?secret_access_key=FAKE-SECRET-123&sse_c_key=QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWY%3D&region=x";
        let e = format!("{:#}", parse(raw).unwrap_err());
        assert!(e.contains("not accepted in the URL"), "{e}");
        for shown in [e, redact(raw)] {
            assert!(!shown.contains("FAKE-SECRET-123"), "{shown}");
            assert!(!shown.contains("QUJDREVG"), "{shown}");
        }
        assert!(redact(raw).contains("region=x"));
        let e = format!("{:#}", parse("sftp://u:FAKE-PW-999@h/x").unwrap_err());
        assert!(!e.contains("FAKE-PW-999") && e.contains("SSH key"), "{e}");
        let e = format!("{:#}", parse("b2://u:FAKE-PW-999@bkt/x").unwrap_err());
        assert!(!e.contains("FAKE-PW-999"), "{e}");
    }

    #[test]
    fn secrets_from_config_are_registered_and_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("s3.ini");
        std::fs::write(&cfg, "key=AKFAKE\nsecret=FAKE-SECRET-456\n").unwrap();
        let s = super::parse("s3://b/k", Some(&cfg)).unwrap().unwrap();
        assert_eq!(s.setting("access_key_id"), Some("AKFAKE"));
        assert_eq!(s.setting("secret_access_key"), Some("FAKE-SECRET-456"));
        let dbg = format!("{s:?} {s}");
        assert!(!dbg.contains("FAKE-SECRET-456"), "{dbg}");
        assert_eq!(
            crate::secrets::scrub("echo FAKE-SECRET-456 back"),
            "echo *** back"
        );
    }

    #[test]
    fn with_key_rewrites_display() {
        let s = p("s3://b/dir/out.json?region=r");
        let part = s.with_key("dir/out.split-1.json");
        assert_eq!(part.to_string(), "s3://b/dir/out.split-1.json?region=r");
        let f = p("fs:///tmp/a/out.json");
        assert_eq!(f.with_key("tmp/a/x").to_string(), "fs:///tmp/a/x");
    }

    #[test]
    fn sse_c_key_forms() {
        use base64::Engine;
        let key = [7u8; 32];
        let b64 = base64::engine::general_purpose::STANDARD.encode(key);
        let dir = tempfile::tempdir().unwrap();
        let raw = dir.path().join("raw.key");
        std::fs::write(&raw, key).unwrap();
        let s = p(&format!("s3://b/k?sse_c_key_file={}", raw.display()));
        let (alg, k) = s.sse_c().unwrap().unwrap();
        assert_eq!(alg, "AES256");
        assert_eq!(k, key);
        let txt = dir.path().join("b64.key");
        std::fs::write(&txt, format!("{b64}\n")).unwrap();
        let s = p(&format!("s3://b/k?sse_c_key_file={}", txt.display()));
        assert_eq!(s.sse_c().unwrap().unwrap().1, key);
        let cfg = dir.path().join("c.ini");
        std::fs::write(&cfg, format!("sse_c_key={b64}\n")).unwrap();
        let s = super::parse("s3://b/k", Some(&cfg)).unwrap().unwrap();
        assert_eq!(s.sse_c().unwrap().unwrap().1, key);
        std::fs::write(&cfg, "sse_c_key=QUJD\n").unwrap();
        let short = super::parse("s3://b/k", Some(&cfg)).unwrap().unwrap();
        assert!(short.sse_c().is_err());
        assert!(p("s3://b/k?sse_c_algorithm=AES256").sse_c().is_err());
        assert!(p("s3://b/k").sse_c().unwrap().is_none());
    }

    #[test]
    fn config_file_sections_and_precedence() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("storage.ini");
        std::fs::write(
            &cfg,
            "region=top\nkey=AKTOP\n[s3]\nregion=from-general\nendpoint=http://general\n[s3.special]\nendpoint=http://special\n",
        )
        .unwrap();
        let s = resolve_settings(Scheme::S3, "special", BTreeMap::new(), Some(&cfg)).unwrap();
        assert_eq!(s["endpoint"], "http://special");
        assert_eq!(s["region"], "from-general");
        assert_eq!(s["access_key_id"], "AKTOP");
        let mut q = BTreeMap::new();
        q.insert("region".to_string(), "from-url".to_string());
        let s = resolve_settings(Scheme::S3, "other", q, Some(&cfg)).unwrap();
        assert_eq!(s["endpoint"], "http://general");
        assert_eq!(s["region"], "from-url");
    }

    #[cfg(feature = "opendal")]
    #[tokio::test(flavor = "multi_thread")]
    async fn copy_tree_between_fs_dirs_is_byte_exact() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src");
        std::fs::create_dir_all(src.join("_cluster")).unwrap();
        std::fs::create_dir_all(src.join("g/h")).unwrap();
        std::fs::write(src.join("a.data.json.gz"), [0x1f, 0x8b, 0, 1, 2, 255]).unwrap();
        std::fs::write(src.join("_cluster/policy.json"), "{}").unwrap();
        std::fs::write(src.join("g/h/b.data.split-0.json"), "x\n".repeat(1000)).unwrap();
        let s = parse_dir(&src.display().to_string(), None).unwrap();
        let t = parse_dir(&format!("fs://{}/dst", d.path().display()), None).unwrap();
        assert!(t.key.ends_with('/'));
        let r = copy_tree(&s, &t, false, 2).await.unwrap();
        assert_eq!(r.files, 3);
        assert_eq!(r.bytes, 6 + 2 + 2000);
        for f in [
            "a.data.json.gz",
            "_cluster/policy.json",
            "g/h/b.data.split-0.json",
        ] {
            assert_eq!(
                std::fs::read(src.join(f)).unwrap(),
                std::fs::read(d.path().join("dst").join(f)).unwrap()
            );
        }
        let e = copy_tree(&s, &t, false, 2).await.unwrap_err();
        assert!(e.to_string().contains("already exists"), "{e}");
        copy_tree(&s, &t, true, 2).await.unwrap();
        let empty = parse_dir(&d.path().join("none").display().to_string(), None).unwrap();
        std::fs::create_dir_all(d.path().join("none")).unwrap();
        assert!(copy_tree(&empty, &t, true, 1).await.is_err());
    }

    #[test]
    fn b2_needs_its_three_settings() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("b2.ini");
        std::fs::write(&cfg, "key=KEYID\n").unwrap();
        let s = super::parse("b2://bucket/k?bucket_id=x", Some(&cfg))
            .unwrap()
            .unwrap();
        assert_eq!(s.setting("application_key_id"), Some("KEYID"));
        assert_eq!(s.setting("bucket_id"), Some("x"));
        if std::env::var_os("B2_APPLICATION_KEY").is_none()
            && std::env::var_os("SEARCHDUMP_B2_APPLICATION_KEY").is_none()
        {
            let e = s.validate().unwrap_err().to_string();
            assert!(e.contains("application_key"), "{e}");
        }
    }
}
