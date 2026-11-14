//! `--input` / `--output` endpoint parsing and credential resolution.
//!
//! * `http://` / `https://` → Elasticsearch/OpenSearch. The last path segment
//!   is the index (a trailing `_search` is ignored); preceding segments are a
//!   proxy base path. A host-only URL addresses the whole cluster.
//! * `$` → stdin/stdout.
//! * `s3://`, `b2://`, `sftp://`, `fs://` / `file://` → the OpenDAL storage path
//!   ([`crate::remote`], L-809a).
//! * anything else → a file path. `csv://` is out of scope.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use percent_encoding::percent_decode_str;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Input => "input",
            Direction::Output => "output",
        }
    }
    pub fn env_prefix(self) -> &'static str {
        match self {
            Direction::Input => "SEARCHDUMP_INPUT",
            Direction::Output => "SEARCHDUMP_OUTPUT",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct EsEndpoint {
    /// Scheme, host, port and base path (ending in `/`), no credentials.
    pub base: Url,
    /// Index name, comma list or pattern. `None` = whole cluster.
    pub index: Option<String>,
    /// Mapping type (`--input-index idx/type`); only meaningful on ES < 7.
    pub doc_type: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Manual `Debug`: the password is never printed (F-08).
impl std::fmt::Debug for EsEndpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EsEndpoint")
            .field("base", &self.base.as_str())
            .field("index", &self.index)
            .field("doc_type", &self.doc_type)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Es(EsEndpoint),
    Stdio,
    File(PathBuf),
    /// An object or file reached through OpenDAL (L-809a).
    Remote(crate::remote::Remote),
}

impl Endpoint {
    pub fn is_es(&self) -> bool {
        matches!(self, Endpoint::Es(_))
    }
}

/// Render a URL for logging with its password masked.
pub fn redacted_url(url: &Url) -> String {
    let mut redacted = url.clone();
    if url.password().is_some() {
        let _ = redacted.set_password(Some("***"));
    }
    redacted.to_string()
}

/// Mask `user:pass@` in an arbitrary endpoint string.
pub fn redacted_endpoint(raw: &str) -> String {
    if crate::remote::Scheme::of(raw).is_some() {
        return crate::remote::redact(raw);
    }
    match Url::parse(raw) {
        // A password with `/`, `?` or `#` can make a URL parse with the
        // password inside the host:port/path (`http://u:12/x@h`): mask the
        // raw text whenever userinfo was not recognised but `@` is present.
        Ok(url)
            if matches!(url.scheme(), "http" | "https")
                && (url.password().is_some() || !raw.contains('@')) =>
        {
            redacted_url(&url)
        }
        // Not a parseable http(s) URL (a typo, a scheme-less
        // `user:pw@host:9200/idx` that became a file path, ...): still mask
        // anything that looks like `user:password@` (F-06).
        _ => mask_userinfo(raw),
    }
}

/// Replace the password in `[scheme://]user:password@...` with `***`.
/// Everything between the first `:` of the authority and the LAST `@` is
/// masked, so a password containing `@`, `/` or spaces never leaks.
pub(crate) fn mask_userinfo(raw: &str) -> String {
    let is_scheme = |s: &str| {
        let mut c = s.chars();
        c.next().is_some_and(|f| f.is_ascii_alphabetic())
            && c.all(|ch| ch.is_ascii_alphanumeric() || "+.-".contains(ch))
    };
    let (scheme, rest) = match raw.find("://") {
        Some(i) if is_scheme(&raw[..i]) => raw.split_at(i + 3),
        _ => ("", raw),
    };
    let Some(at) = rest.rfind('@') else {
        return raw.to_string();
    };
    match rest[..at].find(':') {
        Some(colon) => format!("{scheme}{}:***{}", &rest[..colon], &rest[at..]),
        None => raw.to_string(),
    }
}

/// Replace every command-line argument that carries `user:password@` with
/// its masked form inside `msg` (error messages may quote a file path that
/// was really a mistyped URL; F-06).
pub fn scrub_args(msg: &str, args: &[String]) -> String {
    let mut out = msg.to_string();
    for a in args {
        let masked = mask_userinfo(a);
        if masked != *a && !a.is_empty() {
            out = out.replace(a.as_str(), &masked);
        }
        // `--input=user:pw@...`
        if let Some((_, v)) = a.split_once('=') {
            let masked = mask_userinfo(v);
            if masked != v && !v.is_empty() {
                out = out.replace(v, &masked);
            }
        }
    }
    out
}

/// `user:pw@host...` without a scheme is read as a FILE path; warn, since the
/// user probably meant a URL (F-06, BRK-CLI-008).
fn looks_like_url_without_scheme(raw: &str) -> bool {
    let head = raw.split('/').next().unwrap_or("");
    head.contains('@')
        || (head.contains(':')
            && head
                .rsplit(':')
                .next()
                .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())))
}

/// `user:password@` in a scheme-less endpoint: a `:` before the last `@`
/// (a password may hold `/`, so the whole text counts), unless the value is
/// an explicit path (`/...`, `./...`, `../...`, `~/...`).
fn has_userinfo(raw: &str) -> bool {
    if raw.starts_with(['/', '.', '~']) {
        return false;
    }
    raw.rfind('@').is_some_and(|at| raw[..at].contains(':'))
}

/// A mistyped cluster URL that would otherwise be read or written as a file
/// (BRK-CLI-008): `http:/host`, `http:host`, `http//host`, ` http://host`
/// (whitespace), `htp://host` (unknown scheme). The text returned never
/// quotes the value; only a recognisable scheme name is named.
fn url_typo(raw: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    let trimmed = lower.trim();
    if trimmed != lower && (trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Some("is a URL with leading or trailing whitespace".into());
    }
    for scheme in ["https", "http"] {
        if let Some(rest) = lower.strip_prefix(scheme)
            && (rest.starts_with(':') || rest.starts_with("//"))
        {
            return Some(format!(
                "starts with '{scheme}' but not '{scheme}://', so it looks like a mistyped URL"
            ));
        }
    }
    let (head, _) = raw.split_once("://")?;
    let is_scheme = {
        let mut c = head.chars();
        c.next().is_some_and(|f| f.is_ascii_alphabetic())
            && c.all(|ch| ch.is_ascii_alphanumeric() || "+.-".contains(ch))
    };
    is_scheme.then(|| {
        let shown: String = head.chars().take(16).collect();
        format!("uses the unsupported scheme '{shown}://' (only http:// and https:// are clusters; s3://, b2://, sftp://, fs:// and file:// are storage URLs)")
    })
}

fn decode_component(raw: &str, what: &str, redact: bool) -> Result<String> {
    percent_decode_str(raw)
        .decode_utf8()
        .map(|d| d.into_owned())
        .map_err(|err| {
            if redact {
                anyhow!("Failed to percent-decode {what}: {err}")
            } else {
                anyhow!("Failed to percent-decode {what} '{raw}': {err}")
            }
        })
}

fn split_index_type(spec: &str) -> (Option<String>, Option<String>) {
    let mut parts = spec.splitn(2, '/');
    let index = parts.next().filter(|s| !s.is_empty()).map(str::to_string);
    let ty = parts.next().filter(|s| !s.is_empty()).map(str::to_string);
    (index, ty)
}

/// Parse an endpoint string. `index_override` is `--input-index` /
/// `--output-index` (`index[/type]`).
pub fn parse(raw: &str, dir: Direction, index_override: Option<&str>) -> Result<Endpoint> {
    let flag = dir.as_str();
    if raw == "$" {
        if index_override.is_some() {
            log::warn!("--{flag}-index is ignored for stdio");
        }
        return Ok(Endpoint::Stdio);
    }
    let lower = raw.to_ascii_lowercase();
    let storage_config = crate::remote::config_file(dir);
    if let Some(spec) = crate::remote::parse(raw, storage_config.as_deref())
        .map_err(|e| anyhow!("--{flag} {e:#}"))?
    {
        if index_override.is_some() {
            log::warn!("--{flag}-index is ignored for storage URLs");
        }
        if spec.key.is_empty() || spec.key.ends_with('/') {
            bail!(
                "--{flag} {spec}: name an object or file, not a bucket or directory (e.g. {}://.../dump.json)",
                spec.scheme.name()
            );
        }
        #[cfg(not(feature = "opendal"))]
        return Err(spec.unsupported());
        #[cfg(feature = "opendal")]
        {
            spec.validate().map_err(|e| anyhow!("--{flag} {e:#}"))?;
            return Ok(Endpoint::Remote(std::sync::Arc::new(spec)));
        }
    }
    if lower.starts_with("csv://") {
        bail!("--{flag} csv: CSV transports are not supported by searchdump");
    }
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        if raw.is_empty() {
            bail!("--{flag} must not be empty");
        }
        if let Some(problem) = url_typo(raw) {
            // never quote the value: it may hold a password
            bail!(
                "--{flag} {problem}; write it as http://host:port/index or https://..., or start it with ./ if it really is a file"
            );
        }
        if index_override.is_some() {
            log::warn!("--{flag}-index is ignored for file endpoints");
        }
        let creds = has_userinfo(raw);
        if creds || looks_like_url_without_scheme(raw) {
            if dir == Direction::Output && creds {
                bail!(
                    "--output looks like a URL with credentials (user:password@host) but has no http:// or https://, so it would be written as a FILE whose name holds the password; add http:// or https://, or start it with ./ if it really is a file"
                );
            }
            log::warn!(
                "--{flag} {} is treated as a FILE path; add http:// or https:// if it is a cluster (start it with ./ to silence this)",
                mask_userinfo(raw)
            );
        }
        return Ok(Endpoint::File(PathBuf::from(raw)));
    }

    let encode_hint = || {
        format!(
            "if the password contains '#', '?', '/' or '@', percent-encode it (%23, %3F, %2F, %40) or pass it with --{flag}-password"
        )
    };
    let url = match Url::parse(raw) {
        Ok(u) => u,
        // never quote the URL: the text after the scheme may be a password
        Err(e) if raw.contains('@') => {
            bail!("Failed to parse --{flag} URL: {e}; {}", encode_hint())
        }
        Err(e) => return Err(e).with_context(|| format!("Failed to parse --{flag} URL")),
    };
    if url.host_str().is_none_or(str::is_empty) {
        bail!("--{flag} URL has no host");
    }
    // `@` without a recognised password: possibly a password with `/`, `?`
    // or `#` that spilled into the path or query (`http://u:/pw?x@h`), so no
    // message may quote path or query text (L-505, url_props finding).
    let quiet = url.password().is_none() && raw.contains('@');
    // `http://u:12#x@h/i` parses as host `u`, port 12 and a fragment: the
    // dump would silently go to the wrong host with no credentials (L-645).
    if quiet && (url.fragment().is_some() || url.query().is_some_and(|q| q.contains('@'))) {
        bail!(
            "--{flag} URL has '@' after the host, so its user:password@ was not recognised; {}",
            encode_hint()
        );
    }
    if quiet {
        log::warn!(
            "--{flag} URL contains '@' but no user:password@ was recognised; {}",
            encode_hint()
        );
    }

    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let segments = match segments.last() {
        Some(&"_search") => &segments[..segments.len() - 1],
        _ => &segments[..],
    };

    let (mut index, base_segments) = match segments.split_last() {
        Some((last, rest)) => (Some(decode_component(last, "index name", quiet)?), rest),
        None => (None, segments),
    };
    if let Some(i) = &index {
        if i.starts_with('_') && quiet {
            bail!("--{flag} URL must point to an index (or the cluster root), not an API endpoint");
        }
        if i.starts_with('_') {
            bail!(
                "--{flag} URL must point to an index (or the cluster root), not an API endpoint ('{i}')"
            );
        }
    }
    let mut doc_type = None;
    if let Some(spec) = index_override {
        let (i, t) = split_index_type(spec);
        index = i;
        doc_type = t;
    }

    let username = match url.username() {
        "" => None,
        u => Some(decode_component(u, "URL username", false)?),
    };
    let password = match url.password() {
        Some(p) => Some(decode_component(p, "URL password", true)?),
        None => None,
    };

    if quiet && url.query().is_some() {
        log::warn!("Ignoring the query string in --{flag} URL; use --{flag}-params");
    } else if let Some(query) = url.query() {
        let names = query
            .split('&')
            .map(|p| p.split('=').next().unwrap_or(p))
            .collect::<Vec<_>>()
            .join(", ");
        log::warn!(
            "Ignoring query string parameters in --{flag} URL ({names}); use --{flag}-params"
        );
    }

    let mut base = url.clone();
    let _ = base.set_username("");
    let _ = base.set_password(None);
    base.set_query(None);
    base.set_fragment(None);
    let path = if base_segments.is_empty() {
        "/".to_string()
    } else {
        format!("/{}/", base_segments.join("/"))
    };
    base.set_path(&path);

    Ok(Endpoint::Es(EsEndpoint {
        base,
        index,
        doc_type,
        username,
        password,
    }))
}

/// Parse an `--httpAuthFile` (INI with `user=` and `password=`). Warns when
/// every user on the machine can read it.
pub fn read_auth_file(path: &str) -> Result<(String, String)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read --httpAuthFile '{path}'"))?;
    #[cfg(unix)]
    if let Ok(meta) = std::fs::metadata(path) {
        use std::os::unix::fs::PermissionsExt;
        if let Some(w) = auth_file_mode_warning(path, meta.permissions().mode()) {
            log::warn!("{w}");
        }
    }
    parse_auth_ini(&text).map_err(|e| anyhow::anyhow!("--httpAuthFile '{path}': {e:#}"))
}

/// The warning for an auth file with permission bits `mode`, if it is
/// world-readable.
pub fn auth_file_mode_warning(path: &str, mode: u32) -> Option<String> {
    (mode & 0o004 != 0).then(|| {
        format!(
            "--httpAuthFile '{path}' is readable by every user on this machine (mode {:o}); restrict it with: chmod 600 '{path}'",
            mode & 0o7777
        )
    })
}

/// `user=` and `password=` from the text of an auth file. `\` is a literal
/// character (no INI escapes), surrounding spaces and quotes are removed, a
/// CRLF line end is not part of the value, and `#`/`;` inside a value are
/// kept (Node's `ini` would cut the password there).
pub fn parse_auth_ini(text: &str) -> Result<(String, String)> {
    let opt = ini::ParseOption {
        enabled_escape: false,
        ..Default::default()
    };
    let ini = ini::Ini::load_from_str_opt(text, opt).context("not a valid INI file")?;
    let section = ini.general_section();
    match (section.get("user"), section.get("password")) {
        (Some(u), Some(p)) => Ok((strip_eol(u), strip_eol(p))),
        _ => bail!("Malformed Auth File: it must define both user= and password="),
    }
}

/// A credential without trailing CR/LF: a value pasted from a CRLF file or
/// `$(cat file)` would otherwise carry a `\r` and fail with a confusing 401.
fn strip_eol(v: &str) -> String {
    v.trim_end_matches(['\r', '\n']).to_string()
}

/// Resolve the basic-auth credentials of an endpoint. Precedence, highest
/// first (README "Credentials"; owner decision 2026-11-14, L-645b, a
/// behaviour change from L-645):
///
/// 1. `--httpAuthFile`: when given, its `user=`/`password=` pair is used for
///    both directions and replaces every other source, field by field
///    included;
/// 2. `--{input,output}-username` / `--{input,output}-password`
///    (`--username`/`--password` are aliases of the input pair), each
///    overriding one field;
/// 3. `user:password@` in the URL;
/// 4. `SEARCHDUMP_{INPUT,OUTPUT}_{USERNAME,PASSWORD}` (used when its
///    username is set), only when 2 and 3 together do not give both a
///    username and a password, and then as a pair, with any flag still
///    winning.
///
/// Returns the sources that held a value and were overridden by the auth
/// file (names only, never values), so the caller can say so.
///
/// A trailing CR/LF is removed from every value; an empty password is sent
/// as is (`user:`).
pub fn resolve_auth(
    ep: &mut EsEndpoint,
    dir: Direction,
    auth_file: Option<&(String, String)>,
    env: &dyn Fn(&str) -> Option<String>,
    cli_user: Option<&str>,
    cli_pass: Option<&str>,
) -> Vec<String> {
    let user_key = format!("{}_USERNAME", dir.env_prefix());
    let pass_key = format!("{}_PASSWORD", dir.env_prefix());
    if let Some((u, p)) = auth_file {
        let d = dir.as_str();
        let mut ignored = Vec::new();
        if ep.username.is_some() || ep.password.is_some() {
            ignored.push(format!("the {d} URL credentials"));
        }
        if cli_user.is_some() {
            ignored.push(format!("--{d}-username"));
        }
        if cli_pass.is_some() {
            ignored.push(format!("--{d}-password"));
        }
        if env(&user_key).is_some() || env(&pass_key).is_some() {
            ignored.push(format!("{}_*", dir.env_prefix()));
        }
        ep.username = Some(strip_eol(u));
        ep.password = Some(strip_eol(p));
        return ignored;
    }
    let apply_flags = |ep: &mut EsEndpoint| {
        if let Some(u) = cli_user {
            ep.username = Some(strip_eol(u));
        }
        if let Some(p) = cli_pass {
            ep.password = Some(strip_eol(p));
        }
    };
    apply_flags(ep);
    if ep.username.is_some() && ep.password.is_some() {
        return Vec::new();
    }
    if let Some(u) = env(&user_key) {
        ep.username = Some(strip_eol(&u));
        ep.password = env(&pass_key).as_deref().map(strip_eol);
    }
    apply_flags(ep);
    Vec::new()
}

/// The warning printed when `--httpAuthFile` replaced other credential
/// sources of one direction (names only).
pub fn auth_file_override_warning(dir: Direction, ignored: &[String]) -> Option<String> {
    (!ignored.is_empty()).then(|| {
        format!(
            "--httpAuthFile overrides {} for the {}: the auth file's user and password are used",
            ignored.join(", "),
            dir.as_str()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn es(raw: &str, ov: Option<&str>) -> EsEndpoint {
        match parse(raw, Direction::Input, ov).unwrap() {
            Endpoint::Es(e) => e,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_index_and_base_path() {
        let e = es("http://localhost:9200/my_index", None);
        assert_eq!(e.base.as_str(), "http://localhost:9200/");
        assert_eq!(e.index.as_deref(), Some("my_index"));
        let e = es("https://h/proxy/es/idx/_search", None);
        assert_eq!(e.base.as_str(), "https://h/proxy/es/");
        assert_eq!(e.index.as_deref(), Some("idx"));
    }

    #[test]
    fn host_only_url_has_no_index() {
        let e = es("http://localhost:9200/", None);
        assert_eq!(e.index, None);
        assert_eq!(e.base.as_str(), "http://localhost:9200/");
    }

    #[test]
    fn index_override_replaces_url_index_and_sets_type() {
        let e = es("http://h:9200/ignored", Some("real/doc"));
        assert_eq!(e.index.as_deref(), Some("real"));
        assert_eq!(e.doc_type.as_deref(), Some("doc"));
        let e = es("http://h:9200", Some("only"));
        assert_eq!(e.index.as_deref(), Some("only"));
    }

    #[test]
    fn credentials_are_decoded_and_stripped() {
        let e = es("http://us%40er:p%3Ass@h:9200/i", None);
        assert_eq!(e.username.as_deref(), Some("us@er"));
        assert_eq!(e.password.as_deref(), Some("p:ss"));
        assert_eq!(e.base.as_str(), "http://h:9200/");
    }

    #[test]
    fn rejects_api_paths_and_oos_schemes() {
        assert!(parse("http://h/_cat", Direction::Input, None).is_err());
        assert!(parse("csv:///tmp/x.csv", Direction::Output, None).is_err());
        // L-809a: storage URLs must name an object
        assert!(parse("s3://bucket/", Direction::Output, None).is_err());
        assert!(parse("s3://bucket", Direction::Output, None).is_err());
        #[cfg(feature = "opendal")]
        assert!(matches!(
            parse("s3://bucket/key", Direction::Input, None),
            Ok(Endpoint::Remote(_))
        ));
        #[cfg(not(feature = "opendal"))]
        assert!(parse("s3://bucket/key", Direction::Input, None).is_err());
    }

    #[test]
    fn file_and_stdio() {
        assert_eq!(
            parse("$", Direction::Output, None).unwrap(),
            Endpoint::Stdio
        );
        assert_eq!(
            parse("/tmp/x.json", Direction::Output, None).unwrap(),
            Endpoint::File("/tmp/x.json".into())
        );
    }

    #[test]
    fn auth_precedence_file_then_cli_then_url_then_env() {
        let env = |k: &str| match k {
            "SEARCHDUMP_INPUT_USERNAME" => Some("envu".to_string()),
            "SEARCHDUMP_INPUT_PASSWORD" => Some("envp".to_string()),
            _ => None,
        };
        let file = ("fileu".to_string(), "filep".to_string());

        // L-645b: the auth file beats the URL
        let mut e = es("http://urlu:urlp@h/i", None);
        resolve_auth(&mut e, Direction::Input, Some(&file), &env, None, None);
        assert_eq!(e.username.as_deref(), Some("fileu"));

        let mut e = es("http://h/i", None);
        resolve_auth(&mut e, Direction::Input, Some(&file), &env, None, None);
        assert_eq!(
            (e.username.as_deref(), e.password.as_deref()),
            (Some("fileu"), Some("filep"))
        );

        let mut e = es("http://h/i", None);
        resolve_auth(&mut e, Direction::Input, None, &env, None, None);
        assert_eq!(
            (e.username.as_deref(), e.password.as_deref()),
            (Some("envu"), Some("envp"))
        );

        let mut e = es("http://h/i", None);
        resolve_auth(&mut e, Direction::Output, None, &env, None, None);
        assert_eq!(e.username, None);

        let mut e = es("http://urlu:urlp@h/i", None);
        resolve_auth(
            &mut e,
            Direction::Input,
            None,
            &env,
            Some("cli"),
            Some("clip"),
        );
        assert_eq!(
            (e.username.as_deref(), e.password.as_deref()),
            (Some("cli"), Some("clip"))
        );
    }

    #[test]
    fn auth_file_requires_both_keys() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("ok.ini");
        std::fs::write(&ok, "user=alice\npassword=s3cr=t\n").unwrap();
        assert_eq!(
            read_auth_file(ok.to_str().unwrap()).unwrap(),
            ("alice".into(), "s3cr=t".into())
        );
        let bad = dir.path().join("bad.ini");
        std::fs::write(&bad, "user=alice\n").unwrap();
        let err = read_auth_file(bad.to_str().unwrap()).unwrap_err();
        assert!(err.to_string().contains("Malformed Auth File"));
    }

    /// url, auth file?, env?, flag user, flag password => (user, password)
    type Case<'a> = (
        &'a str,
        bool,
        bool,
        Option<&'a str>,
        Option<&'a str>,
        (Option<String>, Option<String>),
    );

    /// L-645b (owner decision 2026-11-14): the auth file, when given, beats
    /// every other source in both directions; without it, per-direction flags
    /// beat the URL, the URL beats env, and env fills in only an incomplete
    /// pair (L-645 / D-4).
    #[test]
    fn auth_precedence_matrix_for_both_directions() {
        let env = |k: &str| match k {
            "SEARCHDUMP_INPUT_USERNAME" => Some("env-input-u".to_string()),
            "SEARCHDUMP_INPUT_PASSWORD" => Some("FAKE-env-input\r\n".to_string()),
            "SEARCHDUMP_OUTPUT_USERNAME" => Some("env-output-u".to_string()),
            "SEARCHDUMP_OUTPUT_PASSWORD" => Some("FAKE-env-output".to_string()),
            _ => None,
        };
        let no_env = |_: &str| None;
        let file = ("file-u".to_string(), "FAKE-file\r\n".to_string());
        let pair = |e: &EsEndpoint| (e.username.clone(), e.password.clone());
        let p = |u: &str, pw: &str| (Some(u.to_string()), Some(pw.to_string()));
        for dir in [Direction::Input, Direction::Output] {
            let d = dir.as_str();
            let env_pair = p(&format!("env-{d}-u"), &format!("FAKE-env-{d}"));
            let file_pair = p("file-u", "FAKE-file");
            let cases: Vec<Case> = vec![
                ("http://h/i", false, false, None, None, (None, None)),
                ("http://h/i", false, true, None, None, env_pair.clone()),
                // the auth file wins over everything, as a pair
                ("http://h/i", true, true, None, None, file_pair.clone()),
                ("http://h/i", true, false, None, None, file_pair.clone()),
                (
                    "http://url-u:FAKE-url@h/i",
                    true,
                    true,
                    None,
                    None,
                    file_pair.clone(),
                ),
                (
                    "http://url-u@h/i",
                    true,
                    false,
                    None,
                    None,
                    file_pair.clone(),
                ),
                (
                    "http://url-u:FAKE-url@h/i",
                    true,
                    true,
                    Some("flag-u"),
                    Some("FAKE-flag"),
                    file_pair.clone(),
                ),
                (
                    "http://url-u:FAKE-url@h/i",
                    true,
                    false,
                    None,
                    Some("FAKE-flag"),
                    file_pair.clone(),
                ),
                (
                    "http://h/i",
                    true,
                    false,
                    Some("flag-u"),
                    None,
                    file_pair.clone(),
                ),
                (
                    "http://h/i",
                    true,
                    true,
                    Some("flag-u"),
                    Some(""),
                    file_pair.clone(),
                ),
                // without an auth file: the URL beats env
                (
                    "http://url-u:FAKE-url@h/i",
                    false,
                    true,
                    None,
                    None,
                    p("url-u", "FAKE-url"),
                ),
                (
                    "http://url-u@h/i",
                    false,
                    false,
                    None,
                    None,
                    (Some("url-u".into()), None),
                ),
                // flags beat the URL and env, field by field
                (
                    "http://url-u:FAKE-url@h/i",
                    false,
                    true,
                    Some("flag-u"),
                    Some("FAKE-flag"),
                    p("flag-u", "FAKE-flag"),
                ),
                (
                    "http://url-u:FAKE-url@h/i",
                    false,
                    true,
                    None,
                    Some("FAKE-flag"),
                    p("url-u", "FAKE-flag"),
                ),
                (
                    "http://url-u:FAKE-url@h/i",
                    false,
                    true,
                    Some("flag-u"),
                    None,
                    p("flag-u", "FAKE-url"),
                ),
                (
                    "http://h/i",
                    false,
                    true,
                    Some("flag-u"),
                    None,
                    p("flag-u", &format!("FAKE-env-{d}")),
                ),
                (
                    "http://h/i",
                    false,
                    false,
                    Some("flag-u"),
                    Some("FAKE-flag\r"),
                    p("flag-u", "FAKE-flag"),
                ),
                // a complete pair from the flags needs nothing else
                (
                    "http://h/i",
                    false,
                    true,
                    Some("flag-u"),
                    Some(""),
                    p("flag-u", ""),
                ),
            ];
            for (url, use_file, use_env, fu, fp, want) in cases {
                let mut e = es(url, None);
                let envf: &dyn Fn(&str) -> Option<String> = if use_env { &env } else { &no_env };
                resolve_auth(&mut e, dir, use_file.then_some(&file), envf, fu, fp);
                assert_eq!(
                    pair(&e),
                    want,
                    "{d}: {url} file={use_file} env={use_env} {fu:?} {fp:?}"
                );
            }
        }
    }

    /// L-645b: the auth file names (never shows) the sources it replaced.
    #[test]
    fn auth_file_override_is_reported_without_values() {
        let env = |k: &str| match k {
            "SEARCHDUMP_OUTPUT_USERNAME" => Some("env-u".to_string()),
            _ => None,
        };
        let file = ("file-u".to_string(), "FAKE-file".to_string());
        let mut e = es("http://url-u:FAKE-url@h/i", None);
        let ignored = resolve_auth(
            &mut e,
            Direction::Output,
            Some(&file),
            &env,
            Some("flag-u"),
            Some("FAKE-flag"),
        );
        assert_eq!(
            ignored,
            vec![
                "the output URL credentials",
                "--output-username",
                "--output-password",
                "SEARCHDUMP_OUTPUT_*"
            ]
        );
        let w = auth_file_override_warning(Direction::Output, &ignored).unwrap();
        for secret in ["FAKE-url", "FAKE-flag", "FAKE-file", "flag-u", "env-u"] {
            assert!(!w.contains(secret), "{w}");
        }
        let mut e = es("http://h/i", None);
        let none = resolve_auth(&mut e, Direction::Input, Some(&file), &env, None, None);
        assert!(none.is_empty());
        assert_eq!(auth_file_override_warning(Direction::Input, &none), None);
        let mut e = es("http://url-u:FAKE-url@h/i", None);
        assert!(resolve_auth(&mut e, Direction::Input, None, &env, Some("x"), None).is_empty());
    }

    /// TST-BRK-CLI-006: hostile characters survive the auth file and the
    /// URL intact; CRLF is trimmed; `\` is literal.
    #[test]
    fn auth_values_keep_special_characters() {
        let hostile = r"FAKE:@%#/?é ü\x";
        let ini = format!("user=u ser\r\npassword=\"{hostile}\"\r\n");
        assert_eq!(
            parse_auth_ini(&ini).unwrap(),
            ("u ser".into(), hostile.into())
        );
        let ini = "user=u\npassword=FAKE;semi#hash\\back\n";
        assert_eq!(parse_auth_ini(ini).unwrap().1, "FAKE;semi#hash\\back");
        assert_eq!(parse_auth_ini("user=u\npassword=\n").unwrap().1, "");
        let enc =
            percent_encoding::utf8_percent_encode(hostile, percent_encoding::NON_ALPHANUMERIC);
        let e = es(&format!("http://us%40er:{enc}@h:9200/i"), None);
        assert_eq!(e.username.as_deref(), Some("us@er"));
        assert_eq!(e.password.as_deref(), Some(hostile));
        let e = es("http://u:FAKE pw@h:9200/i", None);
        assert_eq!(e.password.as_deref(), Some("FAKE pw"));
    }

    #[test]
    fn world_readable_auth_file_is_warned_about() {
        assert_eq!(auth_file_mode_warning("/a", 0o100600), None);
        assert_eq!(auth_file_mode_warning("/a", 0o100640), None);
        let w = auth_file_mode_warning("/a", 0o100644).unwrap();
        assert!(
            w.contains("readable by every user")
                && w.contains("chmod 600 '/a'")
                && w.contains("644"),
            "{w}"
        );
    }

    /// `#`, `?` or `/` unencoded in a URL password: a clear error that never
    /// quotes the password, instead of a dump to the wrong host.
    #[test]
    fn unencoded_url_password_is_a_clear_error_without_the_password() {
        for raw in [
            "http://u:FAKE#pw@h:9200/i",
            "http://u:FAKE?pw@h:9200/i",
            "http://u:FAKE/pw@h:9200/i",
            "http://u:1234#FAKEpw@h:9200/i",
            "http://u:1234?FAKEpw@h:9200/i",
        ] {
            for dir in [Direction::Input, Direction::Output] {
                let e = format!("{:#}", parse(raw, dir, None).unwrap_err());
                assert!(!e.contains("FAKE"), "{raw}: {e}");
                assert!(
                    e.contains("percent-encode") && e.contains("%23"),
                    "{raw}: {e}"
                );
                assert!(
                    e.contains(&format!("--{}-password", dir.as_str())),
                    "{raw}: {e}"
                );
            }
        }
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::*;

    /// F-06: unparseable or scheme-less endpoints are still masked.
    #[test]
    fn errors_never_quote_a_password_that_spilled_into_the_path() {
        // L-505: url_props minimal failing input
        let raw = "http://a:/Qz%E5?\\\u{e9}K#@ziclmqln/us";
        for dir in [Direction::Input, Direction::Output] {
            let e = parse(raw, dir, None).unwrap_err();
            assert!(!format!("{e:#}").contains("Qz"), "{e:#}");
        }
        let e = parse("http://a:/_Qz?x@h/i", Direction::Input, None).unwrap_err();
        assert!(!format!("{e:#}").contains("Qz"), "{e:#}");
    }

    #[test]
    fn redacted_endpoint_masks_userinfo_without_a_valid_url() {
        assert_eq!(
            redacted_endpoint("elastic:s3cr3t@host:9200/idx"),
            "elastic:***@host:9200/idx"
        );
        assert_eq!(
            redacted_endpoint("http://u:p w@bad host/idx"),
            "http://u:***@bad host/idx"
        );
        assert_eq!(redacted_endpoint("/tmp/dump.json"), "/tmp/dump.json");
        assert_eq!(
            redacted_endpoint("https://u:pw@h:9200/i"),
            "https://u:***@h:9200/i"
        );
        assert!(looks_like_url_without_scheme("elastic:pw@host:9200/idx"));
        assert!(looks_like_url_without_scheme("localhost:9200/idx"));
        assert!(!looks_like_url_without_scheme("/tmp/x.json"));
        assert!(!looks_like_url_without_scheme("dump.json"));
    }

    /// L-646 (TST-BRK-CLI-008): mistyped URLs are usage errors that never
    /// quote the value; uppercase schemes are URLs; `./` forces a file.
    #[test]
    fn mistyped_and_scheme_less_urls() {
        for raw in [
            "http:/u:FAKE-pw@h:9200/i",
            "HTTP:/h:9200/i",
            "https:h:9200/i",
            "http//h:9200/i",
            "Https//u:FAKE-pw@h/i",
            " http://u:FAKE-pw@h:9200/i",
            "\thttps://h/i",
            "htp://u:FAKE-pw@h:9200/i",
            "elasticsearch://h:9200/i",
        ] {
            for dir in [Direction::Input, Direction::Output] {
                let e = format!("{:#}", parse(raw, dir, None).unwrap_err());
                assert!(!e.contains("FAKE"), "{raw:?}: {e}");
                assert!(
                    e.contains("http://host:port/index") && e.contains("./"),
                    "{raw:?}: {e}"
                );
            }
        }
        assert!(url_typo("htp://h").unwrap().contains("'htp://'"));
        // credentials in a scheme-less output would land in a file name
        let e = format!(
            "{:#}",
            parse("elastic:FAKE-pw@h:9200", Direction::Output, None).unwrap_err()
        );
        assert!(!e.contains("FAKE") && e.contains("http://"), "{e}");
        // ... an input only warns (reading cannot leak) and stays a file
        assert_eq!(
            parse("elastic:FAKE-pw@h:9200/i", Direction::Input, None).unwrap(),
            Endpoint::File("elastic:FAKE-pw@h:9200/i".into())
        );
        for (raw, dir) in [
            ("h:9200", Direction::Output),
            ("./elastic:FAKE-pw@h:9200", Direction::Output),
            ("./htp://h", Direction::Output),
            ("http.json", Direction::Output),
            ("https_backup/dump.json", Direction::Input),
            ("/tmp/a:b@c.json", Direction::Output),
        ] {
            assert!(
                matches!(parse(raw, dir, None), Ok(Endpoint::File(_))),
                "{raw}"
            );
        }
        for raw in ["HTTP://h:9200/i", "Https://h/i", "hTTp://u:p@h/i"] {
            assert!(
                parse(raw, Direction::Output, None).unwrap().is_es(),
                "{raw}"
            );
        }
        assert!(has_userinfo("u:p@h:9200/x"));
        assert!(has_userinfo("u:p/x@h")); // a password with '/'
        assert!(has_userinfo("dir/a:b@c.json")); // false positive: ./ escapes it
        assert!(has_userinfo("h:9200/a@b")); // ambiguous (password "9200/a"?): refused, ./ escapes
        assert!(!has_userinfo("./u:p@h"));
        assert!(!has_userinfo("/tmp/a:b@c.json"));
        let e = format!(
            "{:#}",
            parse("elastic:FAKE/pw@h:9200", Direction::Output, None).unwrap_err()
        );
        assert!(!e.contains("FAKE"), "{e}");
    }
}

/// L-503 (TST-API-082): URL parsing and redaction properties.
#[cfg(test)]
mod url_props {
    use super::*;
    use proptest::prelude::*;

    fn cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000)
    }

    /// Passwords start with a marker that appears nowhere else in the URL.
    fn password() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just(':'),
                Just('@'),
                Just('%'),
                Just('#'),
                Just('/'),
                Just('?'),
                Just(' '),
                Just('é'),
                Just('\\'),
                proptest::char::range('0', '9'),
                proptest::char::range('A', 'Z'),
            ],
            0..10,
        )
        .prop_map(|v| format!("Qz{}", v.into_iter().collect::<String>()))
        .prop_flat_map(|p| ("[0-9]{0,3}/?", Just(p)).prop_map(|(d, p)| format!("{d}{p}")))
    }

    fn pct(s: &str) -> String {
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

        /// No redacted rendering ever contains the password, raw or
        /// percent-encoded, whatever the URL looks like; parsing never panics
        /// and its errors never quote the password either.
        #[test]
        fn redaction_never_shows_the_password(
            scheme in prop_oneof![Just("http://"), Just("https://"), Just("HTTP://"), Just(""), Just("http:/")],
            user in "[a-z]{0,6}",
            pass in password(),
            encode in any::<bool>(),
            host in prop_oneof![Just("localhost".to_string()), Just("10.0.0.1".to_string()), Just("[::1]".to_string()), "[a-z]{1,8}", Just("bad host".to_string())],
            port in prop_oneof![Just(String::new()), (1u16..65535).prop_map(|p| format!(":{p}"))],
            path in prop_oneof![Just(String::new()), "/[a-z]{1,6}", "/[a-z]{1,4}/[a-z]{1,4}", Just("/i?x=1".to_string())],
        ) {
            let p = if encode { pct(&pass) } else { pass.clone() };
            let raw = format!("{scheme}{user}:{p}@{host}{port}{path}");
            let red = redacted_endpoint(&raw);
            for secret in [pass.as_str(), pct(&pass).as_str(), "Qz"] {
                prop_assert!(!red.contains(secret), "{raw} -> {red}");
            }
            for dir in [Direction::Input, Direction::Output] {
                if let Err(e) = parse(&raw, dir, None) {
                    let msg = scrub_args(&format!("{e:#}"), std::slice::from_ref(&raw));
                    prop_assert!(!msg.contains("Qz"), "{raw} -> {msg}");
                }
            }
        }

        /// Arbitrary strings never make parsing or redaction panic.
        #[test]
        fn arbitrary_endpoints_never_panic(raw in "\\PC{0,40}", idx in proptest::option::of("\\PC{0,10}")) {
            let _ = redacted_endpoint(&raw);
            let _ = parse(&raw, Direction::Input, idx.as_deref());
            let _ = parse(&raw, Direction::Output, idx.as_deref());
            let _ = scrub_args(&raw, std::slice::from_ref(&raw));
        }
    }
}
