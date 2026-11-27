//! The parent's own cluster client: listing, stats, probes and retries.

use crate::cli::Args;
use crate::sizing;
use crate::tls::ClusterTls;
use anyhow::{Context, Result, anyhow, bail};
use http::Method;
use searchdump_rs::endpoint::read_auth_file;
use searchdump_rs::http::gate::RequestGate;
use searchdump_rs::http::retry::error_chain;
use searchdump_rs::http::{Body, ClientOptions, EsClient};
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

// ----------------------------------------------------------------------------- cluster

/// Base URL without credentials, plus credentials: `--httpAuthFile` wins (L-645b), then URL
/// userinfo, then env.
pub(crate) fn split_url(
    raw: &str,
    env_prefix: &str,
    auth_file: Option<&(String, String)>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Url, Option<String>, Option<String>)> {
    let mut url = Url::parse(raw).with_context(|| {
        format!(
            "invalid URL '{}'",
            searchdump_rs::endpoint::redacted_endpoint(raw)
        )
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        bail!("URL must start with http:// or https://");
    }
    let dec = |s: &str| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    };
    let mut user = (!url.username().is_empty()).then(|| dec(url.username()));
    let mut pass = url.password().map(dec);
    let _ = url.set_username("");
    let _ = url.set_password(None);
    if !url.path().ends_with('/') {
        let p = format!("{}/", url.path());
        url.set_path(&p);
    }
    if let Some((u, p)) = auth_file {
        // L-645b (owner decision 2026-11-14): the auth file wins over the URL and env
        user = Some(u.trim_end_matches(['\r', '\n']).to_string());
        pass = Some(p.trim_end_matches(['\r', '\n']).to_string());
    } else if (user.is_none() || pass.is_none())
        && let Some(u) = env(&format!("{env_prefix}_USERNAME"))
    {
        user = Some(u.trim_end_matches(['\r', '\n']).to_string());
        pass = env(&format!("{env_prefix}_PASSWORD"))
            .map(|p| p.trim_end_matches(['\r', '\n']).to_string());
    }
    Ok((url, user, pass))
}

/// L3-008 (1.3-D): how many times the parent repeats a read-only call (`GET`/`HEAD`, or a
/// `POST .../_refresh` / `.../_count`) the cluster answered with HTTP 429 or 503. A small heap's
/// real-memory parent breaker refuses even a 0-byte `_refresh` until the next GC (seen on
/// OpenSearch 3 at 256m after a load); without a retry the parent failed an index whose child
/// had written every document. Requests that change the cluster are not repeated here.
pub(crate) const PARENT_429_RETRIES: u32 = 6;

pub(crate) fn parent_call_retryable(method: &Method, segs: &[&str], status: u16) -> bool {
    if status != 429 && status != 503 {
        return false;
    }
    *method == Method::GET
        || *method == Method::HEAD
        || (*method == Method::POST && matches!(segs.last(), Some(&"_refresh") | Some(&"_count")))
}

/// 0.25 s, doubling, at most 4 s (about 12 s over all retries).
pub(crate) fn parent_retry_delay(attempt: u32) -> Duration {
    Duration::from_millis((250u64 << attempt.min(4)).min(4000))
}

pub(crate) struct Cluster {
    client: EsClient,
    /// What children get as --input/--output (as the user gave it, without a trailing slash).
    pub(crate) child_base: String,
}

impl Cluster {
    pub(crate) fn new(raw: &str, env_prefix: &str, args: &Args, tls: &ClusterTls) -> Result<Self> {
        let auth = args
            .http_auth_file
            .as_deref()
            .map(read_auth_file)
            .transpose()?;
        let (base, username, password) = split_url(raw, env_prefix, auth.as_ref(), &|k| {
            searchdump_rs::envnames::var(k)
        })?;
        if tls.insecure {
            // searchdump's warning (multi has no logger, so it is printed here)
            eprintln!(
                "warning: TLS certificate verification is DISABLED for {}",
                searchdump_rs::endpoint::redacted_url(&base)
            );
        }
        let opts = ClientOptions {
            base,
            username,
            password,
            headers: vec![],
            params: vec![],
            tls: tls.options(),
            timeout: Some(Duration::from_secs(300)),
            idle_timeout: None,
            compress: false,
            max_sockets: None,
            debug: false,
            no_proxy: false,
        };
        Ok(Self {
            client: EsClient::new(&opts, RequestGate::new(None, None))?,
            child_base: raw.trim_end_matches('/').to_string(),
        })
    }

    pub(crate) fn index_url(&self, index: &str) -> String {
        let enc = percent_encoding::utf8_percent_encode(index, percent_encoding::NON_ALPHANUMERIC)
            .to_string();
        format!("{}/{enc}", self.child_base)
    }

    async fn call(
        &self,
        method: Method,
        segs: &[&str],
        query: &[(&str, &str)],
        body: Body,
    ) -> Result<(u16, Vec<u8>)> {
        let mut attempt = 0u32;
        loop {
            let resp = self
                .client
                .send(method.clone(), segs, query, false, body.clone())
                .await
                .map_err(|e| anyhow!("{method} /{}: {}", segs.join("/"), error_chain(&e)))?;
            let status = resp.status().as_u16();
            let bytes = resp.bytes().await?.to_vec();
            if attempt < PARENT_429_RETRIES && parent_call_retryable(&method, segs, status) {
                let delay = parent_retry_delay(attempt);
                eprintln!(
                    "note: {method} /{} -> HTTP {status} (the cluster is busy or its memory is full); retrying in {delay:?}",
                    segs.join("/")
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
                continue;
            }
            return Ok((status, bytes));
        }
    }

    async fn ok_json(
        &self,
        method: Method,
        segs: &[&str],
        query: &[(&str, &str)],
        body: Body,
    ) -> Result<serde_json::Value> {
        let (status, bytes) = self.call(method.clone(), segs, query, body).await?;
        if !(200..300).contains(&status) {
            let detail = String::from_utf8_lossy(&bytes[..bytes.len().min(2000)]).into_owned();
            let hint = if status == 401 {
                " (credentials rejected: check SEARCHDUMP_*_USERNAME/PASSWORD or --httpAuthFile)"
            } else {
                ""
            };
            bail!(
                "{method} /{} -> HTTP {status}: {detail}{hint}",
                segs.join("/")
            );
        }
        if bytes.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Open indices (closed ones cannot be dumped), sorted by name.
    pub(crate) async fn list_indices(&self) -> Result<Vec<String>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_cat", "indices"],
                &[
                    ("format", "json"),
                    ("h", "index,status"),
                    ("expand_wildcards", "all"),
                    ("s", "index"),
                ],
                Body::Empty,
            )
            .await?;
        let mut out = vec![];
        for row in v
            .as_array()
            .ok_or_else(|| anyhow!("unexpected _cat/indices response"))?
        {
            let name = row
                .get("index")
                .and_then(|x| x.as_str())
                .unwrap_or_default();
            let status = row.get("status").and_then(|x| x.as_str()).unwrap_or("open");
            if name.is_empty() {
                continue;
            }
            if status != "open" {
                eprintln!("note: skipping closed index {name}");
                continue;
            }
            out.push(name.to_string());
        }
        out.sort();
        Ok(out)
    }

    /// (alias, index) pairs, sorted.
    pub(crate) async fn list_aliases(&self) -> Result<Vec<(String, String)>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_cat", "aliases"],
                &[("format", "json"), ("h", "alias,index")],
                Body::Empty,
            )
            .await?;
        let mut out: Vec<(String, String)> = v
            .as_array()
            .ok_or_else(|| anyhow!("unexpected _cat/aliases response"))?
            .iter()
            .filter_map(|row| {
                let a = row.get("alias")?.as_str()?;
                let i = row.get("index")?.as_str()?;
                Some((a.to_string(), i.to_string()))
            })
            .collect();
        out.sort();
        Ok(out)
    }

    /// Data streams and their backing indices, sorted by name.
    pub(crate) async fn list_data_streams(&self) -> Result<Vec<(String, Vec<String>)>> {
        let v = self
            .ok_json(Method::GET, &["_data_stream"], &[], Body::Empty)
            .await
            .context("--matchType datastream: listing data streams (needs Elasticsearch 7.9+ or OpenSearch 1.0+)")?;
        let mut out: Vec<(String, Vec<String>)> = v
            .get("data_streams")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow!("unexpected _data_stream response"))?
            .iter()
            .filter_map(|ds| {
                let name = ds.get("name")?.as_str()?.to_string();
                let members = ds
                    .get("indices")
                    .and_then(|i| i.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|i| i.get("index_name")?.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                Some((name, members))
            })
            .collect();
        out.sort();
        Ok(out)
    }

    /// One data stream's description (`GET _data_stream/NAME`), or None if there is none.
    pub(crate) async fn data_stream(&self, name: &str) -> Result<Option<serde_json::Value>> {
        let (status, bytes) = self
            .call(Method::GET, &["_data_stream", name], &[], Body::Empty)
            .await?;
        match status {
            200..=299 => {
                let v: serde_json::Value = serde_json::from_slice(&bytes)?;
                Ok(v.get("data_streams")
                    .and_then(|d| d.as_array())
                    .and_then(|a| {
                        a.iter()
                            .find(|d| d.get("name").and_then(|n| n.as_str()) == Some(name))
                    })
                    .cloned())
            }
            404 => Ok(None),
            s => bail!(
                "GET /_data_stream/{name} -> HTTP {s}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(500)])
            ),
        }
    }

    pub(crate) async fn exists(&self, index: &str) -> Result<bool> {
        let (status, bytes) = self
            .call(Method::GET, &[index, "_settings"], &[], Body::Empty)
            .await?;
        match status {
            200..=299 => Ok(true),
            404 => Ok(false),
            401 => bail!("GET /{index}/_settings -> HTTP 401: credentials rejected"),
            s => bail!(
                "GET /{index}/_settings -> HTTP {s}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(500)])
            ),
        }
    }

    pub(crate) async fn count(&self, index: &str, refresh: bool) -> Result<u64> {
        if refresh {
            self.ok_json(Method::POST, &[index, "_refresh"], &[], Body::Empty)
                .await?;
        }
        let v = self
            .ok_json(Method::GET, &[index, "_count"], &[], Body::Empty)
            .await?;
        v.get("count")
            .and_then(|c| c.as_u64())
            .ok_or_else(|| anyhow!("no count in /{index}/_count response"))
    }

    /// 1.2: primary docs and stored bytes of every open index in ONE request
    /// (`_stats/docs,store?level=indices`), replacing 1.1-10's per-index `_stats` call.
    pub(crate) async fn index_stats(&self) -> Result<BTreeMap<String, sizing::Stats>> {
        let v = self
            .ok_json(
                Method::GET,
                &["_stats", "docs,store"],
                &[
                    ("level", "indices"),
                    ("expand_wildcards", "open,hidden"),
                    (
                        "filter_path",
                        "indices.*.primaries.docs.count,indices.*.primaries.store.size_in_bytes",
                    ),
                ],
                Body::Empty,
            )
            .await?;
        let mut out = BTreeMap::new();
        if let Some(m) = v.get("indices").and_then(|i| i.as_object()) {
            for (name, s) in m {
                let p = &s["primaries"];
                out.insert(
                    name.clone(),
                    (
                        p["docs"]["count"].as_u64().unwrap_or(0),
                        p["store"]["size_in_bytes"].as_u64().unwrap_or(0),
                    ),
                );
            }
        }
        Ok(out)
    }

    /// The stats of one target (a data stream: its backing indices summed), for a target the
    /// one `index_stats` call did not name.
    pub(crate) async fn target_stats(&self, name: &str) -> Option<sizing::Stats> {
        let v = self
            .ok_json(
                Method::GET,
                &[name, "_stats", "docs,store"],
                &[("filter_path", "_all.primaries")],
                Body::Empty,
            )
            .await
            .ok()?;
        let p = &v["_all"]["primaries"];
        Some((
            p["docs"]["count"].as_u64().unwrap_or(0),
            p["store"]["size_in_bytes"].as_u64().unwrap_or(0),
        ))
    }

    /// 1.2: the node probe (once per run, by the parent).
    pub(crate) async fn probe_limits(&self) -> searchdump_rs::limits::Limits {
        let host = Url::parse(&self.child_base)
            .ok()
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_default();
        searchdump_rs::limits::probe(
            &self.client,
            &host,
            None,
            searchdump_rs::limits::Need {
                max_content_length: true,
                thread_pools: true,
            },
        )
        .await
    }

    pub(crate) async fn put_mapping(
        &self,
        index: &str,
        mappings: &serde_json::Value,
    ) -> Result<()> {
        self.ok_json(Method::PUT, &[index, "_mapping"], &[], Body::json(mappings))
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1.2: `--serverLimits=off` (and an unprobed / fallback run) gives every child exactly the
    /// 1.1.0 data flags: `--limit=1000` (fewer for large documents, the 1.1-10 rule) and
    /// `--maxBulkBytes=16mb`; `--noDefaultDataFlags` gives no sizes at all.
    #[test]
    fn parent_retries_only_read_only_calls_on_429_and_503() {
        assert!(parent_call_retryable(
            &Method::POST,
            &["pmc", "_refresh"],
            429
        ));
        assert!(parent_call_retryable(&Method::GET, &["pmc", "_count"], 503));
        assert!(parent_call_retryable(&Method::HEAD, &["pmc"], 429));
        assert!(!parent_call_retryable(
            &Method::POST,
            &["pmc", "_refresh"],
            500
        ));
        assert!(!parent_call_retryable(&Method::PUT, &["pmc"], 429));
        assert!(!parent_call_retryable(&Method::DELETE, &["pmc"], 429));
        assert!(!parent_call_retryable(&Method::POST, &["_bulk"], 429));
        assert!(!parent_call_retryable(
            &Method::GET,
            &["pmc", "_settings"],
            200
        ));
        assert_eq!(parent_retry_delay(0), Duration::from_millis(250));
        assert_eq!(parent_retry_delay(3), Duration::from_secs(2));
        assert_eq!(parent_retry_delay(9), Duration::from_secs(4));
        let total: Duration = (0..PARENT_429_RETRIES).map(parent_retry_delay).sum();
        assert!(total <= Duration::from_secs(16), "{total:?}");
    }

    #[test]
    fn credentials_order_and_redaction() {
        let env = |k: &str| match k {
            "SEARCHDUMP_INPUT_USERNAME" => Some("envu".to_string()),
            "SEARCHDUMP_INPUT_PASSWORD" => Some("envp\n".to_string()),
            _ => None,
        };
        let (u, user, pass) = split_url("https://h:9200", "SEARCHDUMP_INPUT", None, &env).unwrap();
        assert_eq!(u.as_str(), "https://h:9200/");
        assert_eq!(
            (user.as_deref(), pass.as_deref()),
            (Some("envu"), Some("envp"))
        );
        let (u, user, pass) = split_url(
            "https://a:b%40c@h:9200/base",
            "SEARCHDUMP_INPUT",
            None,
            &env,
        )
        .unwrap();
        assert_eq!(u.as_str(), "https://h:9200/base/");
        assert_eq!((user.as_deref(), pass.as_deref()), (Some("a"), Some("b@c")));
        let file = ("fu".to_string(), "fp".to_string());
        let (_, user, _) =
            split_url("https://h:9200", "SEARCHDUMP_INPUT", Some(&file), &env).unwrap();
        assert_eq!(user.as_deref(), Some("fu"));
        // L-645b: the auth file also beats URL userinfo
        let (u, user, pass) =
            split_url("https://a:b@h:9200", "SEARCHDUMP_INPUT", Some(&file), &env).unwrap();
        assert_eq!(u.as_str(), "https://h:9200/");
        assert_eq!((user.as_deref(), pass.as_deref()), (Some("fu"), Some("fp")));
        assert!(split_url("ftp://h", "SEARCHDUMP_INPUT", None, &env).is_err());
    }
}
