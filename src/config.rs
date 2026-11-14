//! Turn parsed CLI flags into clients, policies and endpoint settings.

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value as JsonValue;

use crate::cli::{Cli, RetryBackoffArg};
use crate::endpoint::{self, Direction, EsEndpoint};
use crate::http::gate::{IntervalLimiter, RequestGate};
use crate::http::tls::TlsOptions;
use crate::http::{Backoff, ClientOptions, EsClient, RetryOn, RetryPolicy};

/// Parse a JSON-object flag (`--headers`, `--params`, ...) into string pairs.
/// Non-string values are rendered as JSON.
pub fn parse_json_pairs(flag: &str, raw: &str) -> Result<Vec<(String, String)>> {
    let v: JsonValue =
        serde_json::from_str(raw).with_context(|| format!("--{flag} must be a JSON object"))?;
    let JsonValue::Object(map) = v else {
        bail!("--{flag} must be a JSON object");
    };
    Ok(map
        .into_iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| {
            let s = match v {
                JsonValue::String(s) => s,
                other => other.to_string(),
            };
            (k, s)
        })
        .collect())
}

fn direction_pick<'a>(
    dir: Direction,
    input: &'a Option<String>,
    output: &'a Option<String>,
) -> &'a Option<String> {
    match dir {
        Direction::Input => input,
        Direction::Output => output,
    }
}

/// Headers in precedence order: `--headers`, then `--{dir}-headers`.
pub fn headers_for(cli: &Cli, dir: Direction) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    if let Some(h) = &cli.headers {
        out.extend(parse_json_pairs("headers", h)?);
    }
    if let Some(h) = direction_pick(dir, &cli.input_headers, &cli.output_headers) {
        out.extend(parse_json_pairs(&format!("{}-headers", dir.as_str()), h)?);
    }
    for (name, value) in &out {
        check_header(name, value)?;
    }
    Ok(out)
}

/// Header names must be RFC 7230 tokens; values visible ASCII, space or tab.
/// CR/LF/NUL would allow header injection (TST-INJ-011). The value is never
/// echoed, since it may be a secret.
fn check_header(name: &str, value: &str) -> Result<()> {
    let token = |c: char| c.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(c);
    if name.is_empty() || !name.chars().all(token) {
        return Err(anyhow::Error::new(crate::UsageError(format!(
            "invalid header name {:?}: only letters, digits and !#$%&'*+-.^_`|~ are allowed",
            name.escape_debug().to_string()
        ))));
    }
    if !value.chars().all(|c| c == '\t' || (' '..='~').contains(&c)) {
        return Err(anyhow::Error::new(crate::UsageError(format!(
            "invalid value for header '{name}': control characters (CR, LF, NUL, ...) and non-ASCII are not allowed"
        ))));
    }
    Ok(())
}

/// `--{dir}-params`, falling back to `--params`.
pub fn params_for(cli: &Cli, dir: Direction) -> Result<Vec<(String, String)>> {
    if let Some(p) = direction_pick(dir, &cli.input_params, &cli.output_params) {
        return parse_json_pairs(&format!("{}-params", dir.as_str()), p);
    }
    match &cli.params {
        Some(p) => parse_json_pairs("params", p),
        None => Ok(vec![]),
    }
}

/// TLS settings. The shared `--ca/--cert/--key/--pass` win over the
/// direction-specific flags (elasticdump assignment order).
pub fn tls_for(cli: &Cli, dir: Direction, env: &dyn Fn(&str) -> Option<String>) -> TlsOptions {
    let (ca, cert, key, pass, insecure) = match dir {
        Direction::Input => (
            cli.input_ca.clone().or_else(|| cli.ca_file.clone()),
            &cli.input_cert,
            &cli.input_key,
            &cli.input_pass,
            cli.input_insecure,
        ),
        Direction::Output => (
            cli.output_ca.clone(),
            &cli.output_cert,
            &cli.output_key,
            &cli.output_pass,
            cli.output_insecure,
        ),
    };
    let node_insecure = env("NODE_TLS_REJECT_UNAUTHORIZED").as_deref() == Some("0");
    TlsOptions {
        ca: cli.ca.clone().or(ca).map(Into::into),
        cert: cli.cert.clone().or_else(|| cert.clone()).map(Into::into),
        key: cli.key.clone().or_else(|| key.clone()).map(Into::into),
        pass: cli.pass.clone().or_else(|| pass.clone()),
        insecure: cli.insecure.unwrap_or(false) || insecure.unwrap_or(false) || node_insecure,
    }
}

/// `--requestTimeout` (seconds; extension): a deadline for the whole request, body included.
/// `--timeout` is not a total deadline: like Node's `request` library it bounds silence, so a
/// slow but progressing response still succeeds (TST-BRK-NET-005; see [`idle_timeout`]).
pub fn request_timeout(cli: &Cli) -> Option<Duration> {
    match cli.request_timeout_secs {
        Some(0) | None => None,
        Some(s) => Some(Duration::from_secs(s)),
    }
}

/// The longest silence on a connection while connecting, waiting for or reading a response
/// (TST-BRK-NET-004/005): `--timeout` in milliseconds (Node's flag: the `request` library's
/// connect/read-idle timeout) when given and non-zero, else `--idleTimeout` in seconds
/// (default 300; 0 disables).
pub fn idle_timeout(cli: &Cli) -> Option<Duration> {
    match cli.timeout {
        Some(ms) if ms > 0 => Some(Duration::from_millis(ms)),
        _ => (cli.idle_timeout_secs > 0).then(|| Duration::from_secs(cli.idle_timeout_secs)),
    }
}

/// `--retryAttempts N` retries N times. With 0 (the default), requests are
/// still retried 4 times (elasticdump's effective `maxAttempts: 5`) unless
/// `--noImplicitRetry` is given.
pub fn retry_policy(cli: &Cli) -> Result<RetryPolicy> {
    let attempts = if cli.retry_attempts > 0 {
        cli.retry_attempts
    } else if cli.no_implicit_retry() {
        0
    } else {
        4
    };
    let retry_on = match &cli.retry_on {
        Some(spec) => RetryOn::parse(spec)?,
        None => RetryOn::default(),
    };
    Ok(RetryPolicy {
        attempts,
        base_delay: Duration::from_millis(cli.retry_delay_ms),
        backoff: match cli.retry_backoff {
            RetryBackoffArg::Fixed => Backoff::Fixed,
            RetryBackoffArg::Exponential => Backoff::Exponential,
        },
        retry_on,
        max_response_bytes: (cli.max_response_mb > 0).then_some(cli.max_response_mb << 20),
    })
}

/// One admission gate shared by every request of the run.
pub fn request_gate(cli: &Cli) -> RequestGate {
    let interval = if cli.interval_cap.is_some() || cli.concurrency_interval.is_some() {
        Some(IntervalLimiter::new(
            cli.interval_cap.unwrap_or(5),
            Duration::from_millis(cli.concurrency_interval.unwrap_or(5000)),
            cli.carryover_concurrency_count.unwrap_or(true),
        ))
    } else {
        None
    };
    RequestGate::new(cli.max_sockets, interval)
}

/// Resolve credentials for an ES endpoint: `--httpAuthFile` over
/// `--{input,output}-username/-password` over the URL over env (L-645b; see
/// [`endpoint::resolve_auth`]). Warns, naming the sources only, when the auth
/// file replaced credentials given another way.
pub fn resolve_endpoint_auth(
    cli: &Cli,
    ep: &mut EsEndpoint,
    dir: Direction,
    auth_file: Option<&(String, String)>,
) {
    let env = |k: &str| crate::envnames::var(k);
    let (u, p) = match dir {
        Direction::Input => (cli.input_username.as_deref(), cli.input_password.as_deref()),
        Direction::Output => (
            cli.output_username.as_deref(),
            cli.output_password.as_deref(),
        ),
    };
    let ignored = endpoint::resolve_auth(ep, dir, auth_file, &env, u, p);
    if let Some(w) = endpoint::auth_file_override_warning(dir, &ignored) {
        log::warn!("{w}");
    }
}

pub fn build_client(
    cli: &Cli,
    ep: &EsEndpoint,
    dir: Direction,
    gate: RequestGate,
) -> Result<EsClient> {
    let env = |k: &str| crate::envnames::var(k);
    let tls = tls_for(cli, dir, &env);
    if (tls.cert.is_some() || tls.key.is_some()) && cli.tls_auth != Some(true) {
        log::warn!(
            "Client certificate flags are applied without --tlsAuth (elasticdump ignores them unless --tlsAuth is set)"
        );
    }
    if tls.cert.is_some() != tls.key.is_some() {
        return Err(anyhow!(
            "--{d}-cert and --{d}-key (or --cert/--key) must be given together",
            d = dir.as_str()
        ));
    }
    if std::env::var("NODE_TLS_REJECT_UNAUTHORIZED").as_deref() == Ok("0") {
        log::warn!("NODE_TLS_REJECT_UNAUTHORIZED=0: TLS certificate verification is disabled");
    }
    let opts = ClientOptions {
        base: ep.base.clone(),
        username: ep.username.clone(),
        password: ep.password.clone(),
        headers: headers_for(cli, dir)?,
        params: params_for(cli, dir)?,
        tls,
        timeout: request_timeout(cli),
        idle_timeout: idle_timeout(cli),
        compress: cli.es_compress(),
        max_sockets: cli.max_sockets,
        debug: cli.debug(),
        no_proxy: cli.no_proxy(),
    };
    EsClient::new(&opts, gate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(extra: &[&str]) -> Cli {
        let mut v = vec!["x", "--input", "http://h:9200/i", "--output", "$"];
        v.extend_from_slice(extra);
        Cli::parse_from(v)
    }

    #[test]
    fn json_pairs() {
        let p = parse_json_pairs("params", r#"{"a":"b","n":1,"t":true,"z":null}"#).unwrap();
        assert_eq!(
            p,
            vec![
                ("a".into(), "b".into()),
                ("n".into(), "1".into()),
                ("t".into(), "true".into())
            ]
        );
        assert!(parse_json_pairs("params", "[1]").is_err());
        assert!(parse_json_pairs("params", "nope").is_err());
    }

    #[test]
    fn headers_merge_and_params_fallback() {
        let c = cli(&[
            "--headers",
            r#"{"X-A":"1","X-B":"g"}"#,
            "--input-headers",
            r#"{"X-B":"in"}"#,
            "--params",
            r#"{"p":"all"}"#,
            "--output-params",
            r#"{"o":"out"}"#,
        ]);
        assert_eq!(
            headers_for(&c, Direction::Input).unwrap(),
            vec![
                ("X-A".to_string(), "1".to_string()),
                ("X-B".to_string(), "g".to_string()),
                ("X-B".to_string(), "in".to_string())
            ]
        );
        assert_eq!(headers_for(&c, Direction::Output).unwrap().len(), 2);
        assert_eq!(
            params_for(&c, Direction::Input).unwrap(),
            vec![("p".into(), "all".into())]
        );
        assert_eq!(
            params_for(&c, Direction::Output).unwrap(),
            vec![("o".into(), "out".into())]
        );
    }

    #[test]
    fn tls_precedence() {
        let none = |_: &str| None;
        let c = cli(&[
            "--ca",
            "shared.pem",
            "--input-ca",
            "in.pem",
            "--output-cert",
            "o.crt",
        ]);
        let t = tls_for(&c, Direction::Input, &none);
        assert_eq!(t.ca.unwrap().to_str(), Some("shared.pem"));
        let t = tls_for(&c, Direction::Output, &none);
        assert_eq!(t.cert.unwrap().to_str(), Some("o.crt"));

        let c = cli(&["--caFile", "legacy.pem"]);
        assert_eq!(
            tls_for(&c, Direction::Input, &none).ca.unwrap().to_str(),
            Some("legacy.pem")
        );
        assert!(tls_for(&c, Direction::Output, &none).ca.is_none());

        let c = cli(&["--output-insecure"]);
        assert!(!tls_for(&c, Direction::Input, &none).insecure);
        assert!(tls_for(&c, Direction::Output, &none).insecure);
        let node = |k: &str| (k == "NODE_TLS_REJECT_UNAUTHORIZED").then(|| "0".to_string());
        assert!(tls_for(&cli(&[]), Direction::Input, &node).insecure);
    }

    #[test]
    fn retry_defaults_and_overrides() {
        let p = retry_policy(&cli(&[])).unwrap();
        assert_eq!(p.attempts, 4);
        assert_eq!(p.base_delay, Duration::from_millis(5000));
        assert_eq!(
            retry_policy(&cli(&["--noImplicitRetry"])).unwrap().attempts,
            0
        );
        let p = retry_policy(&cli(&["--retryAttempts", "2", "--retryDelay", "10"])).unwrap();
        assert_eq!((p.attempts, p.base_delay), (2, Duration::from_millis(10)));
        assert!(retry_policy(&cli(&["--retryOn", "bogus"])).is_err());
    }

    #[test]
    fn idle_timeout_defaults_to_five_minutes() {
        assert_eq!(idle_timeout(&cli(&[])), Some(Duration::from_secs(300)));
        assert_eq!(
            idle_timeout(&cli(&["--idleTimeout", "7"])),
            Some(Duration::from_secs(7))
        );
        assert_eq!(idle_timeout(&cli(&["--idleTimeout", "0"])), None);
    }

    #[test]
    fn timeout_is_an_idle_bound_and_request_timeout_a_deadline() {
        assert_eq!(request_timeout(&cli(&[])), None);
        assert_eq!(
            request_timeout(&cli(&["--timeout", "1500", "--requestTimeout", "9"])),
            Some(Duration::from_secs(9))
        );
        assert_eq!(request_timeout(&cli(&["--timeout", "1500"])), None);
        assert_eq!(
            idle_timeout(&cli(&["--timeout", "1500", "--idleTimeout", "7"])),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(
            idle_timeout(&cli(&["--timeout", "0", "--idleTimeout", "7"])),
            Some(Duration::from_secs(7))
        );
    }
}
