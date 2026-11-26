//! Shared helpers for the podman-backed integration suite.
//!
//! Every test reads the cluster location from the environment written by
//! `scripts/it/up.sh` (`IT_ES_URL`, plus TLS/auth material for the secure
//! profile) and talks to the cluster with its own small reqwest client, so the
//! assertions never depend on the code under test.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::{Value, json};

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub fn unique(label: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("it_{}_{}_{}", label, std::process::id(), n)
}

pub fn tmp_path(label: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{}.{}", unique(label), ext))
}

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Secured profile (TLS + basic auth) of either engine.
pub fn is_secure() -> bool {
    env("IT_PROFILE").is_some_and(|p| p.ends_with("-secure"))
}

/// Whether a client certificate authenticates on its own (OpenSearch's demo
/// admin cert). On Elasticsearch the PKI realm is not available, so a
/// client certificate is only presented and verified alongside basic auth.
pub fn cert_auth() -> bool {
    env("IT_CERT_AUTH").as_deref() == Some("1")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    OpenSearch,
    Elasticsearch,
}

impl Engine {
    pub fn parse(s: &str) -> Engine {
        if s.starts_with("elasticsearch") {
            Engine::Elasticsearch
        } else {
            Engine::OpenSearch
        }
    }
}

/// Engine of the cluster under test (from `IT_ENGINE`, written by up.sh).
pub fn engine() -> Engine {
    Engine::parse(&env("IT_ENGINE").unwrap_or_else(|| "opensearch".into()))
}

/// Handle on the cluster under test.
#[derive(Clone)]
pub struct Cluster {
    pub url: String,
    client: reqwest::Client,
    user: Option<String>,
    pass: Option<String>,
    pub engine: Engine,
}

impl Cluster {
    pub fn from_env() -> Self {
        let url = env("IT_ES_URL").unwrap_or_else(|| "http://127.0.0.1:19200".into());
        Self::new(&url, env("IT_ADMIN_USER"), env("IT_ADMIN_PW"), engine())
    }

    /// The second cluster of the cross-engine profile (`IT_PEER_URL`,
    /// `IT_PEER_ENGINE`; security off), if any.
    pub fn peer() -> Option<Self> {
        let url = env("IT_PEER_URL")?;
        let engine = Engine::parse(&env("IT_PEER_ENGINE")?);
        Some(Self::new(&url, None, None, engine))
    }

    fn new(url: &str, user: Option<String>, pass: Option<String>, engine: Engine) -> Self {
        let client = {
            searchdump_rs::http::tls::install_default_provider();
            reqwest::Client::builder()
        }
        .danger_accept_invalid_certs(true)
        .build()
        .expect("harness client");
        Self {
            url: url.trim_end_matches('/').to_string(),
            client,
            user,
            pass,
            engine,
        }
    }

    /// `version.number` from `GET /`.
    pub async fn version(&self) -> String {
        self.ok("GET", "", None).await["version"]["number"]
            .as_str()
            .unwrap()
            .to_string()
    }

    pub async fn major(&self) -> u64 {
        self.version()
            .await
            .split('.')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    /// `(major, minor)` of the cluster version.
    pub async fn vnum(&self) -> (u64, u64) {
        let v = self.version().await;
        let mut it = v.split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0));
        (it.next().unwrap(), it.next().unwrap_or(0))
    }

    /// Elasticsearch OSS build (no X-Pack: no PIT, ILM or security).
    pub async fn is_oss(&self) -> bool {
        self.ok("GET", "", None).await["version"]["build_flavor"] == "oss"
    }

    /// ILM (Elasticsearch with X-Pack) or ISM (OpenSearch) available.
    pub async fn has_policies(&self) -> bool {
        !self.is_oss().await
    }

    /// Point-in-time search: Elasticsearch >= 7.10 with X-Pack, OpenSearch >= 2.4.
    pub async fn has_pit(&self) -> bool {
        let v = self.vnum().await;
        match self.engine {
            Engine::Elasticsearch => v >= (7, 10) && !self.is_oss().await,
            Engine::OpenSearch => v >= (2, 4),
        }
    }

    /// URL with admin basic-auth credentials embedded (for the CLI under test).
    pub fn url_with_auth(&self, path: &str) -> String {
        let mut u =
            url::Url::parse(&format!("{}/{}", self.url, path.trim_start_matches('/'))).unwrap();
        if let (Some(user), Some(pass)) = (&self.user, &self.pass) {
            u.set_username(user).unwrap();
            u.set_password(Some(pass)).unwrap();
        }
        u.to_string()
    }

    /// Bare URL, no credentials.
    pub fn index_url(&self, index: &str) -> String {
        format!("{}/{}", self.url, index)
    }

    /// CLI flags needed to reach this cluster (CA bundle for the secure profile).
    pub fn tls_args(&self, direction: &str) -> Vec<String> {
        match env("IT_CA") {
            Some(ca) if is_secure() && self.url.starts_with("https") => {
                vec![format!("--{direction}-ca"), ca]
            }
            _ => vec![],
        }
    }

    fn req(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}/{}", self.url, path.trim_start_matches('/'));
        let mut rb = self.client.request(method, url);
        if let (Some(u), Some(p)) = (&self.user, &self.pass) {
            rb = rb.basic_auth(u, Some(p));
        }
        rb
    }

    pub async fn call(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let (mut status, mut v) = self.call_once(method, path, body.clone()).await;
        // concurrent first ISM PUTs on a fresh cluster race to create the config index
        // (409 on 2.x, 400 resource_already_exists_exception on 1.3)
        for i in 1..=5 {
            let t = v.to_string();
            if !((status == 409 || status == 400)
                && t.contains("ism-config")
                && t.contains("already exists"))
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200 * i)).await;
            (status, v) = self.call_once(method, path, body.clone()).await;
        }
        (status, v)
    }

    async fn call_once(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let m = reqwest::Method::from_bytes(method.as_bytes()).unwrap();
        let mut rb = self.req(m, path);
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let resp = rb.send().await.expect("harness request");
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let v = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, v)
    }

    pub async fn ok(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, v) = self.call(method, path, body).await;
        assert!(status < 300, "{method} {path} -> {status}: {v}");
        v
    }

    pub async fn bulk_raw(&self, ndjson: String) {
        let resp = self
            .req(reqwest::Method::POST, "_bulk?refresh=true")
            .header("content-type", "application/x-ndjson")
            .body(ndjson)
            .send()
            .await
            .unwrap();
        let v: Value = resp.json().await.unwrap();
        if v["errors"] == json!(true) {
            let failed: Vec<&Value> = v["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|i| i["index"]["status"].as_u64().unwrap_or(0) >= 300)
                .collect();
            panic!("seed bulk failed: {failed:?}");
        }
    }

    pub async fn count(&self, index: &str) -> u64 {
        self.ok("POST", &format!("{index}/_refresh"), None).await;
        let v = self.ok("GET", &format!("{index}/_count"), None).await;
        v["count"].as_u64().unwrap()
    }

    /// Create `dst` with `src`'s mappings and analysis settings (so a data
    /// restore does not depend on dynamic mapping).
    pub async fn copy_shape(&self, src: &str, dst: &str) {
        let m = self.ok("GET", &format!("{src}/_mapping"), None).await;
        let st = self.ok("GET", &format!("{src}/_settings"), None).await;
        let analysis = st[src]["settings"]["index"]["analysis"].clone();
        let mut settings = json!({"number_of_shards": 2, "number_of_replicas": 0});
        if !analysis.is_null() {
            settings["analysis"] = analysis;
        }
        self.delete_index(dst).await;
        self.ok(
            "PUT",
            dst,
            Some(json!({"settings": settings, "mappings": m[src]["mappings"].clone()})),
        )
        .await;
    }

    pub async fn delete_index(&self, index: &str) {
        let _ = self.call("DELETE", index, None).await;
    }

    /// All docs of an index as (id -> (routing, _source)) using a PIT-free
    /// scroll sorted by _doc; independent of the code under test.
    pub async fn docs(
        &self,
        index: &str,
    ) -> std::collections::BTreeMap<String, (Option<String>, Value)> {
        let mut out = std::collections::BTreeMap::new();
        self.ok("POST", &format!("{index}/_refresh"), None).await;
        let mut v = self
            .ok(
                "POST",
                &format!("{index}/_search?scroll=1m"),
                Some(json!({"size": 1000, "sort": ["_doc"]})),
            )
            .await;
        loop {
            let hits = v["hits"]["hits"].as_array().cloned().unwrap_or_default();
            if hits.is_empty() {
                // release the context so open-context assertions (API-010/011) see only the tool's
                if let Some(sid) = v["_scroll_id"].as_str() {
                    let _ = self
                        .call(
                            "DELETE",
                            "_search/scroll",
                            Some(json!({"scroll_id": [sid]})),
                        )
                        .await;
                }
                break;
            }
            for h in hits {
                out.insert(
                    h["_id"].as_str().unwrap().to_string(),
                    (
                        h["_routing"].as_str().map(str::to_string),
                        h["_source"].clone(),
                    ),
                );
            }
            let sid = v["_scroll_id"].as_str().unwrap().to_string();
            v = self
                .ok(
                    "POST",
                    "_search/scroll",
                    Some(json!({"scroll": "1m", "scroll_id": sid})),
                )
                .await;
        }
        out
    }
}

pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_searchdump")
}

/// Run the CLI without `--quiet` (for `--debug` assertions).
pub async fn run_loud(args: Vec<String>) -> Output {
    tokio::task::spawn_blocking(move || {
        Command::new(bin())
            .args(&args)
            .output()
            .expect("spawn searchdump")
    })
    .await
    .unwrap()
}

/// Run the CLI synchronously (on a blocking thread) and return its output.
pub async fn run(args: Vec<String>) -> Output {
    run_env(args, vec![]).await
}

pub async fn run_env(args: Vec<String>, envs: Vec<(String, String)>) -> Output {
    tokio::task::spawn_blocking(move || {
        let mut cmd = Command::new(bin());
        cmd.args(&args);
        // IT_VERBOSE=1 keeps the tool's warnings (retries etc.) in failure output
        if std::env::var("IT_VERBOSE").is_err() {
            cmd.arg("--quiet");
        }
        for (k, v) in envs {
            cmd.env(k, v);
        }
        cmd.output().expect("spawn searchdump")
    })
    .await
    .unwrap()
}

pub fn assert_success(out: &Output) {
    assert!(
        out.status.success(),
        "command failed ({:?})\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

pub fn lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

pub fn s<T: AsRef<str>>(items: &[T]) -> Vec<String> {
    items.iter().map(|i| i.as_ref().to_string()).collect()
}

/// Deterministic seed: `n` docs with routing on every third doc, a few
/// adversarial documents, two shards, and a custom analyzer.
pub async fn seed_index(c: &Cluster, index: &str, n: usize) {
    c.delete_index(index).await;
    c.ok(
        "PUT",
        index,
        Some(json!({
            "settings": {
                "number_of_shards": 2,
                "number_of_replicas": 0,
                "analysis": {"analyzer": {"content": {"type": "custom", "tokenizer": "whitespace", "filter": ["lowercase"]}}}
            },
            "mappings": {
                "properties": {
                    "k": {"type": "keyword"},
                    "n": {"type": "long"},
                    "t": {"type": "text", "analyzer": "content"},
                    "d": {"type": "date"},
                    "obj": {"properties": {"properties": {"type": "keyword"}, "x": {"type": "integer"}}},
                    "nested": {"type": "nested", "properties": {"a": {"type": "keyword"}}},
                    "big": {"type": "keyword"},
                    "neg": {"type": "keyword"}
                }
            }
        })),
    )
    .await;
    let mut nd = String::new();
    for i in 0..n {
        let meta = if i % 3 == 0 {
            json!({"index": {"_index": index, "_id": i.to_string(), "routing": format!("r{}", i % 7)}})
        } else {
            json!({"index": {"_index": index, "_id": i.to_string()}})
        };
        let doc = json!({
            "k": format!("key{i}"),
            "n": i,
            "t": format!("Hello World number {i}"),
            "d": "2026-01-01T00:00:00Z",
            "obj": {"properties": format!("p{}", i % 5), "x": i % 100},
            "nested": [{"a": "x"}, {"a": format!("y{i}")}]
        });
        nd.push_str(&meta.to_string());
        nd.push('\n');
        nd.push_str(&doc.to_string());
        nd.push('\n');
    }
    // Adversarial documents: unicode, escapes, very large integers (kept as raw
    // text), deep nesting, empty object.
    let adversarial = [
        r#"{"k":"unicode-✓-漢字-🚀","t":"line\nbreak \"quoted\" \\ back","n":1}"#,
        r#"{"k":"bigint","n":1,"big":123456789012345678901234567890,"neg":-9223372036854775809}"#,
        r#"{"k":"deep","obj":{"x":1},"deep":{"a":{"b":{"c":{"d":{"e":[1,2,3],"f":null}}}}}}"#,
        r#"{"k":"empty"}"#,
    ];
    for (j, d) in adversarial.iter().enumerate() {
        nd.push_str(&json!({"index": {"_index": index, "_id": format!("adv{j}")}}).to_string());
        nd.push('\n');
        nd.push_str(d);
        nd.push('\n');
    }
    c.bulk_raw(nd).await;
}

/// Raw `_source` text of every line keyed by `_id`, for byte-exact checks.
pub fn raw_sources_by_id(path: &Path) -> std::collections::BTreeMap<String, String> {
    let mut m = std::collections::BTreeMap::new();
    for l in lines(path) {
        let v: Value = serde_json::from_str(&l).unwrap();
        let id = v["_id"].as_str().unwrap().to_string();
        // Slice the raw bytes: re-serialisation would lose big integers.
        let raw = sonic_rs::get(l.as_bytes(), ["_source"]).unwrap();
        m.insert(id, raw.as_raw_str().to_string());
    }
    m
}
