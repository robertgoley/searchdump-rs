//! 1.2-1 (ROADMAP-1.2-PLAN sections 2-4, 7): server-aware sizing, `--serverLimits`.
//!
//! `off`   no probing; the 1.1.0 fixed sizes (the comparison baseline and escape hatch).
//! `auto`  probe once at start-up (read-only stats calls), compute a memory budget from the
//!         smallest data node and only ever **tighten** the default page / bulk / worker sizes
//!         (searchdump-multi may return to the 1.0 page of 5000 small documents on a cluster that
//!         can take it). User-given values win, with a warning when above the detected limit.
//! `min`   the same with half of `auto`'s budget (PROVISIONAL, ROADMAP-1.2 section 9 Q2).
//! `check` probe, print the limits and the sizes `auto` and `min` would use, and exit 0 without
//!         moving data; exit 2 when an explicit size flag exceeds what the cluster can take.
//!
//! Probing never fails a run: a 403 (no `monitor` privilege), a missing `_nodes` API
//! (serverless), hidden fields or a probe slower than [`PROBE_DEADLINE`] fall back to the fixed
//! defaults with one warning. Mid-run HTTP 429 handling (page halving, bulk splitting) is
//! separate (1.2-1c, `retrieval_task::send_page_halving`, `BulkSink::write_chunk`) and applies in
//! every mode.

use std::sync::OnceLock;
use std::time::Duration;

use anyhow::Result;
use reqwest::Method;
use serde_json::{Value, json};

use crate::http::{Body, ClusterInfo, EsClient, Flavor};

/// `--serverLimits` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Auto,
    Min,
    Check,
}

/// The default mode. PROVISIONAL pending Robert (ROADMAP-1.2 section 9 Q1): `auto`, as the plan
/// proposes; switching the release to `off`-by-default is this one constant.
pub const DEFAULT_MODE: Mode = Mode::Auto;

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Auto => "auto",
            Mode::Min => "min",
            Mode::Check => "check",
        }
    }
}

pub fn parse_mode(value: &str) -> std::result::Result<Mode, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "false" | "no" | "0" | "none" => Ok(Mode::Off),
        "auto" | "true" | "yes" | "on" | "1" => Ok(Mode::Auto),
        "min" | "minimum" => Ok(Mode::Min),
        "check" => Ok(Mode::Check),
        other => Err(format!(
            "unknown --serverLimits value '{other}' (use off, auto, min or check)"
        )),
    }
}

/// How long the start-up probe may take before the fixed defaults are used.
pub const PROBE_DEADLINE: Duration = Duration::from_secs(5);

/// AWS OpenSearch Service / legacy Amazon ES: the HTTP payload limit is 10 MiB on the small
/// instance types and 100 MiB on larger ones
/// (<https://docs.aws.amazon.com/opensearch-service/latest/developerguide/limits.html>,
/// "Network limits"). The instance type is not visible to a client, so bulks are capped at the
/// 10 MiB floor unless the user raises `--maxBulkBytes` (PROVISIONAL, section 9 Q5: floor only,
/// no per-instance table).
pub const AWS_PAYLOAD_FLOOR: u64 = 10 << 20;

/// Elasticsearch / OpenSearch default `http.max_content_length`.
pub const DEFAULT_MAX_CONTENT_LENGTH: u64 = 100 << 20;

/// The largest page `auto` gives searchdump-multi (the 1.0 value), so a big cluster gets the
/// beta1 page sizes back.
pub const MULTI_MAX_PAGE: u64 = 5000;

/// Fetched `_source` + JSON framing per stored byte (stored size is compressed). Starts at 3
/// (ROADMAP-1.2 3.1); the 1.2-1b matrix found no case where 3 under-sized a page.
pub const EXPANSION: u64 = 3;

/// Where the limits came from (`source` in the log line, `--statsFile` and the manifest).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Probed,
    AwsTable,
    Fallback403,
    FallbackServerless,
    FallbackHidden,
    FallbackTimeout,
    FallbackError,
    Off,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Probed => "probed",
            Source::AwsTable => "aws-table",
            Source::Fallback403 => "fallback-403",
            Source::FallbackServerless => "fallback-serverless",
            Source::FallbackHidden => "fallback-hidden",
            Source::FallbackTimeout => "fallback-timeout",
            Source::FallbackError => "fallback-error",
            Source::Off => "off",
        }
    }

    pub fn is_fallback(self) -> bool {
        matches!(
            self,
            Source::Fallback403
                | Source::FallbackServerless
                | Source::FallbackHidden
                | Source::FallbackTimeout
                | Source::FallbackError
                | Source::Off
        )
    }
}

/// One node's limits (bytes), as far as the cluster reports them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    /// Holds data (role `d`/`data*`, or no roles reported).
    pub data: bool,
    pub heap_max: u64,
    pub heap_used_pct: Option<u64>,
    pub parent_limit: Option<u64>,
    pub parent_estimated: Option<u64>,
    pub request_limit: Option<u64>,
    pub indexing_pressure: Option<u64>,
    pub max_content_length: Option<u64>,
    pub search_pool: Option<u64>,
    pub write_pool: Option<u64>,
}

impl Node {
    /// Parent-breaker headroom: limit minus what is in use now (95 % of the heap minus the used
    /// heap when the breaker is not reported).
    pub fn parent_free(&self) -> Option<u64> {
        match (self.parent_limit, self.parent_estimated) {
            (Some(l), Some(e)) => Some(l.saturating_sub(e)),
            (Some(l), None) => Some(l),
            _ if self.heap_max > 0 => {
                let used = self.heap_max / 100 * self.heap_used_pct.unwrap_or(50);
                Some((self.heap_max / 100 * 95).saturating_sub(used))
            }
            _ => None,
        }
    }

    /// The request breaker (60 % of the heap by default).
    pub fn request(&self) -> Option<u64> {
        self.request_limit
            .or((self.heap_max > 0).then(|| self.heap_max / 100 * 60))
    }

    /// Indexing pressure (10 % of the heap by default, ES >= 7.9 and OpenSearch).
    pub fn indexing(&self) -> Option<u64> {
        self.indexing_pressure
            .or((self.heap_max > 0).then_some(self.heap_max / 10))
    }
}

/// What the start-up probe found.
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub source: Source,
    pub nodes: Vec<Node>,
    /// AWS OpenSearch Service / Amazon ES host: bulks capped at [`AWS_PAYLOAD_FLOOR`].
    pub aws: bool,
    /// Why the fixed defaults are used (fallbacks), for the log line.
    pub reason: String,
}

impl Limits {
    pub fn fallback(source: Source, reason: impl Into<String>) -> Self {
        Self {
            source,
            nodes: vec![],
            aws: false,
            reason: reason.into(),
        }
    }

    /// Data nodes, or every node when none says it holds data.
    pub fn data_nodes(&self) -> Vec<&Node> {
        let d: Vec<&Node> = self.nodes.iter().filter(|n| n.data).collect();
        if d.is_empty() {
            self.nodes.iter().collect()
        } else {
            d
        }
    }

    /// The data node with the least parent-breaker headroom (behind a load balancer the
    /// coordinating node is unknown, so the smallest is the safe choice).
    pub fn smallest(&self) -> Option<&Node> {
        self.data_nodes()
            .into_iter()
            .filter(|n| n.parent_free().is_some())
            .min_by_key(|n| n.parent_free().unwrap_or(u64::MAX))
    }

    /// The smallest `http.max_content_length` any node reports (default 100 MB).
    pub fn max_content_length(&self) -> u64 {
        self.nodes
            .iter()
            .filter_map(|n| n.max_content_length)
            .min()
            .unwrap_or(DEFAULT_MAX_CONTENT_LENGTH)
    }

    /// The smallest search pool of a data node.
    pub fn search_pool(&self) -> Option<u64> {
        self.data_nodes().iter().filter_map(|n| n.search_pool).min()
    }
}

/// The memory budget for one run, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// One search page's response may use this much.
    pub page_bytes: u64,
    /// All pages in flight at once (slices x concurrency x parallel workers) together.
    pub inflight_bytes: u64,
    /// All bulk requests in flight at once together.
    pub bulk_total_bytes: u64,
    /// Never send a bulk above this (http.max_content_length - 1 MB, or the AWS floor).
    pub bulk_cap_bytes: u64,
    pub search_pool: Option<u64>,
}

/// Budget fractions of the smallest node's parent-breaker headroom: (page, in-flight, bulk).
/// `min` is half of `auto` (PROVISIONAL, section 9 Q2).
fn fractions(mode: Mode) -> (f64, f64, f64) {
    match mode {
        Mode::Min => (0.125, 0.25, 0.25),
        _ => (0.25, 0.5, 0.5),
    }
}

fn frac(v: u64, f: f64) -> u64 {
    (v as f64 * f) as u64
}

/// 1.3 evaluation only (1.3-D+/C/E): opt-in sizing variants, chosen with the hidden environment
/// variable `SEARCHDUMP_LIMITS_EXPERIMENT` (comma-separated). Unset (the default) is exactly the
/// 1.2 behaviour. Not a user-facing setting: kept for measurements until Robert decides.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Experiment {
    /// `floor`: never size from less headroom than a quarter of the parent-breaker limit
    /// (a probe taken while the heap is full of garbage, e.g. right after a load, would
    /// otherwise give a 0 MB page budget).
    pub floor: bool,
    /// `configured`: size from half of the configured parent-breaker limit, ignoring the
    /// momentary estimate (the configured limits do not move with garbage collection).
    pub configured: bool,
    /// `growback`: after a 429 split, let the bulk cap double again after a run of
    /// accepted bulks (see `pipeline::bulk`), never above the starting size.
    pub growback: bool,
    /// `retryfirst`: on a size 429 for a bulk, send the same request once more after the
    /// jittered back-off before splitting it (a real-memory parent breaker often trips on
    /// garbage that the next GC frees; 1.1 retried at the same size and recovered).
    pub retryfirst: bool,
}

pub const EXPERIMENT_ENV: &str = "SEARCHDUMP_LIMITS_EXPERIMENT";

impl Experiment {
    pub fn parse(v: &str) -> Self {
        let mut e = Experiment::default();
        for w in v.split(',').map(str::trim) {
            match w {
                "floor" => e.floor = true,
                "configured" => e.configured = true,
                "growback" => e.growback = true,
                "retryfirst" => e.retryfirst = true,
                _ => {}
            }
        }
        e
    }

    pub fn from_env() -> Self {
        std::env::var(EXPERIMENT_ENV)
            .map(|v| Self::parse(&v))
            .unwrap_or_default()
    }
}

/// The headroom `budget` sizes from: 1.2 uses the momentary parent-breaker headroom.
fn sizing_free(n: &Node, exp: Experiment) -> Option<u64> {
    let free = n.parent_free()?;
    let limit = n
        .parent_limit
        .or((n.heap_max > 0).then(|| n.heap_max / 100 * 95));
    Some(match (exp.configured, exp.floor, limit) {
        (true, _, Some(l)) => l / 2,
        (false, true, Some(l)) => free.max(l / 4),
        _ => free,
    })
}

/// ROADMAP-1.2 3.1. `None` for a fallback (fixed defaults apply).
pub fn budget(l: &Limits, mode: Mode, response_cap: u64) -> Option<Budget> {
    budget_with(l, mode, response_cap, Experiment::from_env())
}

/// [`budget`] with an explicit [`Experiment`] (1.3 evaluation; the default is 1.2).
pub fn budget_with(l: &Limits, mode: Mode, response_cap: u64, exp: Experiment) -> Option<Budget> {
    if l.source.is_fallback() || matches!(mode, Mode::Off) {
        return None;
    }
    let n = l.smallest()?;
    let free = sizing_free(n, exp)?.max(1 << 20);
    let request = n.request().unwrap_or(free);
    let (kp, ki, kb) = fractions(mode);
    let mut page = frac(free.min(request), kp);
    if response_cap > 0 {
        page = page.min(response_cap);
    }
    let indexing = n.indexing().unwrap_or(free);
    let mut cap = l.max_content_length().saturating_sub(1 << 20).max(1 << 20);
    if l.aws {
        cap = cap.min(AWS_PAYLOAD_FLOOR);
    }
    Some(Budget {
        page_bytes: page.max(64 << 10),
        inflight_bytes: frac(free, ki).max(64 << 10),
        bulk_total_bytes: frac(indexing.min(free), kb).max(256 << 10),
        bulk_cap_bytes: cap,
        search_pool: l.search_pool(),
    })
}

/// Documents per page that fit `page_bytes` for documents of `avg_stored` stored bytes:
/// `clamp(page_bytes / (avg_stored x EXPANSION), 1, max_page)`; `max_page` when the size is
/// unknown.
pub fn page_docs(page_bytes: u64, avg_stored: Option<u64>, max_page: u64) -> u64 {
    match avg_stored {
        Some(avg) if avg > 0 => {
            (page_bytes / avg.saturating_mul(EXPANSION).max(1)).clamp(1, max_page.max(1))
        }
        _ => max_page.max(1),
    }
}

/// Bytes per bulk request for `workers` bulks in flight (concurrency x parallel), never above
/// the request cap.
pub fn bulk_bytes(b: &Budget, workers: u64) -> u64 {
    (b.bulk_total_bytes / workers.max(1))
        .min(b.bulk_cap_bytes)
        .max(256 << 10)
}

// ------------------------------------------------------------------- parsing (fixture-tested)

fn bytes_of(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str().and_then(es_bytes))
}

/// An Elasticsearch byte-size setting (`100mb`, `20MB`, `1gb`, `512kb`, `1024b`, `1024`): the
/// units are powers of 1024 there, unlike `bytesize`'s decimal `mb`.
fn es_bytes(s: &str) -> Option<u64> {
    let t = s.trim().to_ascii_lowercase();
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let n: f64 = num.parse().ok()?;
    let mul: u64 = match unit.trim() {
        "" | "b" => 1,
        "k" | "kb" => 1 << 10,
        "m" | "mb" => 1 << 20,
        "g" | "gb" => 1 << 30,
        "t" | "tb" => 1 << 40,
        "p" | "pb" => 1 << 50,
        _ => return None,
    };
    Some((n * mul as f64) as u64)
}

/// Parse `GET _nodes/stats/jvm,breaker[,indexing_pressure]` (filtered or not).
pub fn parse_node_stats(body: &Value) -> Vec<Node> {
    let Some(nodes) = body.get("nodes").and_then(Value::as_object) else {
        return vec![];
    };
    let mut out: Vec<Node> = nodes
        .iter()
        .map(|(id, n)| {
            let roles: Vec<&str> = n
                .get("roles")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let br = |name: &str, field: &str| {
                n.pointer(&format!("/breakers/{name}/{field}"))
                    .and_then(bytes_of)
            };
            Node {
                name: n
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string(),
                data: roles.is_empty() || roles.iter().any(|r| r.starts_with("data") || *r == "d"),
                heap_max: n
                    .pointer("/jvm/mem/heap_max_in_bytes")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                heap_used_pct: n
                    .pointer("/jvm/mem/heap_used_percent")
                    .and_then(Value::as_u64),
                parent_limit: br("parent", "limit_size_in_bytes"),
                parent_estimated: br("parent", "estimated_size_in_bytes"),
                request_limit: br("request", "limit_size_in_bytes"),
                indexing_pressure: n
                    .pointer("/indexing_pressure/memory/limit_in_bytes")
                    .and_then(Value::as_u64)
                    .filter(|v| *v > 0),
                ..Default::default()
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Apply `GET _nodes/settings` (an explicit `http.max_content_length`; none means the 100mb default).
pub fn apply_settings(nodes: &mut [Node], body: &Value) {
    let Some(map) = body.get("nodes").and_then(Value::as_object) else {
        return;
    };
    for (id, n) in map {
        let name = n.get("name").and_then(Value::as_str).unwrap_or(id);
        let v = ["settings", "defaults"].iter().find_map(|k| {
            let s = n.get(*k)?;
            s.pointer("/http/max_content_length")
                .or_else(|| s.get("http.max_content_length"))
                .and_then(bytes_of)
        });
        if let Some(v) = v {
            for node in nodes
                .iter_mut()
                .filter(|x| x.name == name || nodes_len_one(map))
            {
                node.max_content_length = Some(v);
            }
        }
    }
}

fn nodes_len_one(map: &serde_json::Map<String, Value>) -> bool {
    map.len() == 1
}

/// Apply `GET _cat/thread_pool/search,write?h=node_name,name,size&format=json`.
pub fn apply_thread_pools(nodes: &mut [Node], rows: &Value) {
    let Some(rows) = rows.as_array() else {
        return;
    };
    for r in rows {
        let node = r.get("node_name").and_then(Value::as_str).unwrap_or("");
        let pool = r.get("name").and_then(Value::as_str).unwrap_or("");
        let size = r.get("size").and_then(|s| {
            s.as_u64()
                .or_else(|| s.as_str().and_then(|x| x.parse().ok()))
        });
        for n in nodes.iter_mut().filter(|n| n.name == node) {
            match pool {
                "search" => n.search_pool = size,
                "write" => n.write_pool = size,
                _ => {}
            }
        }
    }
}

/// AWS OpenSearch Service / Amazon ES endpoints (no AWS call is made: host name only).
pub fn is_aws_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h.ends_with(".es.amazonaws.com") || (h.contains(".aos.") && h.ends_with(".on.aws"))
}

/// OpenSearch Serverless collection endpoints.
pub fn is_serverless_host(host: &str) -> bool {
    host.to_ascii_lowercase().ends_with(".aoss.amazonaws.com")
}

// ------------------------------------------------------------------- probing

/// The node-stats metric list this version understands (unknown metrics are a 400 on old
/// versions): `indexing_pressure` from Elasticsearch 7.9 and on every OpenSearch.
pub fn stats_metrics(info: Option<&ClusterInfo>) -> &'static str {
    match info {
        Some(i) if i.flavor == Flavor::OpenSearch => "jvm,breaker,indexing_pressure",
        Some(i) if i.version >= semver::Version::new(7, 9, 0) => "jvm,breaker,indexing_pressure",
        Some(_) => "jvm,breaker",
        // unknown version: ask for everything, retry without indexing_pressure on a 400
        None => "jvm,breaker,indexing_pressure",
    }
}

const STATS_FILTER: &str = "nodes.*.name,nodes.*.roles,nodes.*.jvm.mem.heap_max_in_bytes,nodes.*.jvm.mem.heap_used_percent,nodes.*.breakers.parent,nodes.*.breakers.request,nodes.*.indexing_pressure.memory.limit_in_bytes";

async fn get(client: &EsClient, segs: &[&str], query: &[(&str, &str)]) -> Result<(u16, Vec<u8>)> {
    let resp = client
        .send(Method::GET, segs, query, false, Body::Empty)
        .await?;
    let status = resp.status().as_u16();
    Ok((status, resp.bytes().await?.to_vec()))
}

/// Probe the cluster behind `client` (read-only, at most three requests plus the cached
/// `GET /`), never failing: errors become a fallback [`Source`].
/// Which optional probes a run needs: `http.max_content_length` only sizes bulks (a load),
/// the thread pools only the multi worker count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Need {
    pub max_content_length: bool,
    pub thread_pools: bool,
}

pub async fn probe(
    client: &EsClient,
    host: &str,
    info: Option<&ClusterInfo>,
    need: Need,
) -> Limits {
    if is_serverless_host(host) {
        return Limits::fallback(Source::FallbackServerless, "OpenSearch Serverless endpoint");
    }
    let mut l =
        match tokio::time::timeout(PROBE_DEADLINE, probe_inner(client, host, info, need)).await {
            Ok(l) => l,
            Err(_) => Limits::fallback(
                Source::FallbackTimeout,
                format!(
                    "the stats calls took longer than {} s",
                    PROBE_DEADLINE.as_secs()
                ),
            ),
        };
    // the AWS payload floor applies even without node stats
    l.aws = l.aws || is_aws_host(host);
    l
}

async fn probe_inner(
    client: &EsClient,
    host: &str,
    info: Option<&ClusterInfo>,
    need: Need,
) -> Limits {
    let metrics = stats_metrics(info);
    // the three calls run at once (start-up cost target < 100 ms, ROADMAP-1.2 section 3)
    let stats_segs = ["_nodes", "stats", metrics];
    let stats_query = [("filter_path", STATS_FILTER)];
    let stats = get(client, &stats_segs, &stats_query);
    let settings = async {
        if need.max_content_length {
            Some(
                // the nodes info API has no include_defaults (a 400): a node without an explicit
                // http.max_content_length uses the 100mb default (DEFAULT_MAX_CONTENT_LENGTH)
                get(
                    client,
                    &["_nodes", "settings"],
                    &[(
                        "filter_path",
                        "nodes.*.name,nodes.*.settings.http.max_content_length",
                    )],
                )
                .await,
            )
        } else {
            None
        }
    };
    let pools = async {
        if need.thread_pools {
            Some(
                get(
                    client,
                    &["_cat", "thread_pool", "search,write"],
                    &[("h", "node_name,name,size"), ("format", "json")],
                )
                .await,
            )
        } else {
            None
        }
    };
    let (stats, settings, pools) = tokio::join!(stats, settings, pools);
    let stats = match stats {
        // the version was not known (no `GET /` for the probe): an old version refuses the
        // unknown `indexing_pressure` metric with a 400, so ask once more without it
        Ok((400, _)) if info.is_none() => {
            get(client, &["_nodes", "stats", "jvm,breaker"], &stats_query).await
        }
        r => r,
    };
    let (status, body) = match stats {
        Ok(r) => r,
        Err(e) => return Limits::fallback(Source::FallbackError, format!("_nodes/stats: {e:#}")),
    };
    match status {
        200..=299 => {}
        401 | 403 => {
            return Limits::fallback(
                Source::Fallback403,
                format!("HTTP {status} on _nodes/stats: no monitor privilege"),
            );
        }
        404 | 410 | 400 | 501 => {
            return Limits::fallback(
                Source::FallbackServerless,
                format!(
                    "HTTP {status} on _nodes/stats: no node stats API (serverless or a proxy hiding it)"
                ),
            );
        }
        s => return Limits::fallback(Source::FallbackError, format!("HTTP {s} on _nodes/stats")),
    }
    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let mut nodes = parse_node_stats(&v);
    if nodes.is_empty()
        || nodes
            .iter()
            .all(|n| n.heap_max == 0 && n.parent_limit.is_none())
    {
        return Limits::fallback(
            Source::FallbackHidden,
            "_nodes/stats returned no heap or breaker fields",
        );
    }
    if let Some(Ok((200..=299, b))) = settings
        && let Ok(s) = serde_json::from_slice::<Value>(&b)
    {
        apply_settings(&mut nodes, &s);
    }
    if let Some(Ok((200..=299, b))) = pools
        && let Ok(s) = serde_json::from_slice::<Value>(&b)
    {
        apply_thread_pools(&mut nodes, &s);
    }
    let aws = is_aws_host(host);
    Limits {
        source: if aws {
            Source::AwsTable
        } else {
            Source::Probed
        },
        nodes,
        aws,
        reason: String::new(),
    }
}

/// Average stored bytes per primary document of `index` (`GET {index}/_stats/docs,store`).
pub async fn avg_doc_bytes(client: &EsClient, index: &str) -> Option<u64> {
    let (status, body) = get(
        client,
        &[index, "_stats", "docs,store"],
        &[("filter_path", "_all.primaries")],
    )
    .await
    .ok()?;
    if !(200..300).contains(&status) {
        return None;
    }
    let v: Value = serde_json::from_slice(&body).ok()?;
    let p = v.pointer("/_all/primaries")?;
    let docs = p.pointer("/docs/count")?.as_u64()?;
    let bytes = p.pointer("/store/size_in_bytes")?.as_u64()?;
    (docs > 0 && bytes > 0).then(|| bytes / docs)
}

// ------------------------------------------------------------------- reporting

fn mb(b: u64) -> String {
    let m = b as f64 / (1u64 << 20) as f64;
    if m >= 10.0 {
        format!("{m:.0} MB")
    } else {
        format!("{m:.1} MB")
    }
}

/// The detected-limits half of the "server limits:" line.
pub fn describe(l: &Limits) -> String {
    if l.source.is_fallback() {
        return format!("unknown ({}) -> fixed 1.1 defaults", l.reason);
    }
    let data = l.data_nodes();
    let Some(n) = l.smallest() else {
        return "unknown (no usable node) -> fixed 1.1 defaults".into();
    };
    let mut s = format!(
        "{} data node{}, smallest {}: heap {}",
        data.len(),
        if data.len() == 1 { "" } else { "s" },
        n.name,
        mb(n.heap_max)
    );
    if let Some(p) = n.heap_used_pct {
        s.push_str(&format!(" ({p} % used)"));
    }
    if let Some(pl) = n.parent_limit {
        s.push_str(&format!(
            ", parent breaker {} (free {})",
            mb(pl),
            mb(n.parent_free().unwrap_or(0))
        ));
    }
    if let Some(r) = n.request() {
        s.push_str(&format!(", request {}", mb(r)));
    }
    if let Some(i) = n.indexing() {
        s.push_str(&format!(", indexing pressure {}", mb(i)));
    }
    s.push_str(&format!(
        ", http.max_content_length {}",
        mb(l.max_content_length())
    ));
    if let Some(p) = l.search_pool() {
        s.push_str(&format!(", search pool {p}"));
    }
    if l.aws {
        s.push_str(&format!(", AWS payload cap {}", mb(AWS_PAYLOAD_FLOOR)));
    }
    s
}

/// The JSON object for `--statsFile` and the multi manifest (`server_limits`).
pub fn to_json(l: &Limits, mode: Mode, b: Option<&Budget>) -> Value {
    json!({
        "mode": mode.as_str(),
        "source": l.source.as_str(),
        "reason": if l.reason.is_empty() { Value::Null } else { Value::from(l.reason.clone()) },
        "aws": l.aws,
        "nodes": l.nodes.iter().map(|n| json!({
            "name": n.name, "data": n.data, "heap_max": n.heap_max, "heap_used_percent": n.heap_used_pct,
            "parent_limit": n.parent_limit, "parent_estimated": n.parent_estimated,
            "request_limit": n.request_limit, "indexing_pressure": n.indexing_pressure,
            "max_content_length": n.max_content_length, "search_pool": n.search_pool, "write_pool": n.write_pool,
        })).collect::<Vec<_>>(),
        "budget": b.map(|b| json!({
            "page_bytes": b.page_bytes, "inflight_bytes": b.inflight_bytes,
            "bulk_total_bytes": b.bulk_total_bytes, "bulk_cap_bytes": b.bulk_cap_bytes,
            "search_pool": b.search_pool,
        })),
    })
}

/// The `server_limits` object of this run (searchdump `--statsFile`).
static RUN_LIMITS: OnceLock<Value> = OnceLock::new();

pub fn record(v: Value) {
    let _ = RUN_LIMITS.set(v);
}

pub fn recorded() -> Option<&'static Value> {
    RUN_LIMITS.get()
}

/// Flags the user gave explicitly (raw argv scan: clap cannot tell a default from a given
/// value), so `auto`/`min` respect them and `check` can refuse them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Explicit {
    pub limit: bool,
    pub max_bulk_bytes: bool,
    pub max_response_mb: bool,
    pub concurrency: bool,
    pub slices: bool,
}

impl Explicit {
    pub fn from_args<S: AsRef<str>>(args: &[S]) -> Self {
        let mut e = Self::default();
        for a in args {
            let a = a.as_ref();
            if a == "--" {
                break;
            }
            let name = a.split_once('=').map_or(a, |(n, _)| n);
            match name {
                "--limit" => e.limit = true,
                "--maxBulkBytes" => e.max_bulk_bytes = true,
                "--maxResponseMb" => e.max_response_mb = true,
                "--concurrency" => e.concurrency = true,
                "--slices" => e.slices = true,
                _ => {}
            }
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/server-limits")
            .join(name);
        serde_json::from_slice(
            &std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
        )
        .unwrap()
    }

    fn limits(stats: &str) -> Limits {
        let nodes = parse_node_stats(&fixture(stats));
        Limits {
            source: Source::Probed,
            nodes,
            aws: false,
            reason: String::new(),
        }
    }

    #[test]
    fn modes_parse() {
        assert_eq!(parse_mode("auto"), Ok(Mode::Auto));
        assert_eq!(parse_mode("OFF"), Ok(Mode::Off));
        assert_eq!(parse_mode("false"), Ok(Mode::Off));
        assert_eq!(parse_mode("min"), Ok(Mode::Min));
        assert_eq!(parse_mode("check"), Ok(Mode::Check));
        assert!(parse_mode("max").is_err());
        assert_eq!(
            DEFAULT_MODE,
            Mode::Auto,
            "PROVISIONAL default (section 9 Q1)"
        );
    }

    /// Every stored fixture parses to at least one data node with a heap and a parent breaker,
    /// and yields a budget that grows with the heap.
    #[test]
    fn fixtures_parse_and_budget_grows_with_heap() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server-limits");
        let mut seen = 0;
        let mut by_engine: std::collections::BTreeMap<String, Vec<(u64, u64)>> = Default::default();
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".nodes-stats.json") else {
                continue;
            };
            let l = limits(&name);
            assert!(!l.nodes.is_empty(), "{name}");
            let n = l
                .smallest()
                .unwrap_or_else(|| panic!("{name}: no smallest node"));
            assert!(n.heap_max > 0 && n.parent_limit.is_some(), "{name}: {n:?}");
            let b = budget(&l, Mode::Auto, 256 << 20).unwrap();
            let bm = budget(&l, Mode::Min, 256 << 20).unwrap();
            assert!(
                bm.page_bytes <= b.page_bytes && bm.inflight_bytes <= b.inflight_bytes,
                "{name}"
            );
            let (engine, heap) = stem.rsplit_once('-').unwrap();
            by_engine
                .entry(engine.to_string())
                .or_default()
                .push((heap.trim_end_matches('m').parse().unwrap(), b.page_bytes));
            seen += 1;
        }
        assert!(seen >= 7, "fixtures: {seen}");
        for (engine, mut v) in by_engine {
            v.sort();
            for w in v.windows(2) {
                assert!(
                    w[0].1 <= w[1].1 * 2,
                    "{engine}: budget does not shrink much with a bigger heap: {v:?}"
                );
            }
        }
    }

    /// 1.3 evaluation (1.3-D+): the relcmp-20261117 os3/384m dump-fat probe (94 % heap used,
    /// parent 365 MB with 2.3 MB free) sized a 0 MB page; `floor` and `configured` do not, and
    /// the default (no experiment) is the 1.2 result unchanged.
    #[test]
    fn experiments_size_from_configured_limits_not_a_full_heap() {
        let full = Limits {
            source: Source::Probed,
            nodes: vec![Node {
                name: "n".into(),
                data: true,
                heap_max: 384 << 20,
                heap_used_pct: Some(94),
                parent_limit: Some(365 << 20),
                parent_estimated: Some((365 << 20) - (23 << 17)),
                request_limit: Some(230 << 20),
                indexing_pressure: Some(38 << 20),
                ..Default::default()
            }],
            aws: false,
            reason: String::new(),
        };
        let none = budget_with(&full, Mode::Auto, 0, Experiment::default()).unwrap();
        assert_eq!(
            none.page_bytes,
            (23u64 << 17) / 4,
            "1.2: a quarter of 2.9 MB headroom"
        );
        assert!(page_docs(none.page_bytes, Some(79 << 10), MULTI_MAX_PAGE) <= 3);
        let floor = budget_with(&full, Mode::Auto, 0, Experiment::parse("floor")).unwrap();
        assert_eq!(floor.page_bytes, ((365u64 << 20) / 4) / 4);
        assert!(page_docs(floor.page_bytes, Some(79 << 10), MULTI_MAX_PAGE) >= 90);
        let conf = budget_with(&full, Mode::Auto, 0, Experiment::parse("configured")).unwrap();
        assert_eq!(conf.page_bytes, ((365u64 << 20) / 2) / 4);
        // never above the request breaker, and `min` stays half of auto
        assert!(conf.page_bytes <= 230 << 20);
        let conf_min = budget_with(&full, Mode::Min, 0, Experiment::parse("configured")).unwrap();
        assert_eq!(conf_min.page_bytes * 2, conf.page_bytes);
        // with plenty of headroom `floor` changes nothing
        let mut idle = full.clone();
        idle.nodes[0].parent_estimated = Some(100 << 20);
        assert_eq!(
            budget_with(&idle, Mode::Auto, 0, Experiment::parse("floor")),
            budget_with(&idle, Mode::Auto, 0, Experiment::default())
        );
        assert_eq!(
            Experiment::parse(" floor , growback,x"),
            Experiment {
                floor: true,
                growback: true,
                ..Default::default()
            }
        );
        assert_eq!(
            budget_with(&full, Mode::Off, 0, Experiment::parse("floor,retryfirst")),
            None
        );
    }

    /// A 256 MB heap gets a page that fits the 1.1-10 evidence: pmc (~12 KB stored) well under
    /// 1000 per page, fat (~80 KB stored) about 100; a 4 GB heap gets the full multi page.
    #[test]
    fn small_heap_tightens_big_heap_does_not() {
        let small = Limits {
            source: Source::Probed,
            nodes: vec![Node {
                name: "n".into(),
                data: true,
                heap_max: 256 << 20,
                heap_used_pct: Some(60),
                parent_limit: Some(243 << 20),
                parent_estimated: Some(150 << 20),
                request_limit: Some(154 << 20),
                indexing_pressure: Some(25 << 20),
                ..Default::default()
            }],
            aws: false,
            reason: String::new(),
        };
        let b = budget(&small, Mode::Auto, 256 << 20).unwrap();
        assert_eq!(b.page_bytes, (93u64 << 20) / 4);
        let pmc = page_docs(b.page_bytes, Some(12 << 10), MULTI_MAX_PAGE);
        let fat = page_docs(b.page_bytes, Some(80 << 10), MULTI_MAX_PAGE);
        assert!((400..1000).contains(&pmc), "{pmc}");
        assert!((50..150).contains(&fat), "{fat}");
        assert_eq!(
            page_docs(b.page_bytes, Some(1 << 30), MULTI_MAX_PAGE),
            1,
            "huge documents: floor 1"
        );
        let bm = budget(&small, Mode::Min, 256 << 20).unwrap();
        assert_eq!(
            bm.page_bytes * 2,
            b.page_bytes,
            "min = half of auto (PROVISIONAL Q2)"
        );
        // bulk: 25 MB indexing pressure x 0.5 / 2 workers
        assert_eq!(bulk_bytes(&b, 2), (25u64 << 20) / 4);
        let mut big = small.clone();
        big.nodes[0] = Node {
            name: "n".into(),
            data: true,
            heap_max: 4 << 30,
            heap_used_pct: Some(30),
            parent_limit: Some((4u64 << 30) / 100 * 95),
            parent_estimated: Some((4u64 << 30) / 100 * 30),
            request_limit: Some((4u64 << 30) / 100 * 60),
            indexing_pressure: Some((4u64 << 30) / 10),
            ..Default::default()
        };
        let bb = budget(&big, Mode::Auto, 256 << 20).unwrap();
        assert_eq!(
            page_docs(bb.page_bytes, Some(1 << 10), MULTI_MAX_PAGE),
            MULTI_MAX_PAGE
        );
        assert!(
            bulk_bytes(&bb, 2) >= 99 << 20,
            "big cluster: bulks up to the content-length cap"
        );
    }

    #[test]
    fn fallbacks_give_no_budget() {
        for s in [
            Source::Fallback403,
            Source::FallbackServerless,
            Source::FallbackHidden,
            Source::FallbackTimeout,
            Source::FallbackError,
            Source::Off,
        ] {
            assert!(budget(&Limits::fallback(s, "x"), Mode::Auto, 256 << 20).is_none());
        }
        let l = limits("es819-256m.nodes-stats.json");
        assert!(budget(&l, Mode::Off, 256 << 20).is_none());
    }

    #[test]
    fn aws_hosts_cap_bulks_at_the_floor() {
        assert!(is_aws_host("search-x-abc.us-east-1.es.amazonaws.com"));
        assert!(is_aws_host("vpc-x.aos.eu-west-1.on.aws"));
        assert!(!is_aws_host("es.example.com"));
        assert!(is_serverless_host("abc.us-east-1.aoss.amazonaws.com"));
        let mut l = limits("es819-1024m.nodes-stats.json");
        l.aws = true;
        l.source = Source::AwsTable;
        let b = budget(&l, Mode::Auto, 256 << 20).unwrap();
        assert_eq!(b.bulk_cap_bytes, AWS_PAYLOAD_FLOOR);
        assert!(bulk_bytes(&b, 1) <= AWS_PAYLOAD_FLOOR);
    }

    #[test]
    fn settings_and_thread_pools_apply() {
        let mut l = limits("es819-256m.nodes-stats.json");
        let name = l.nodes[0].name.clone();
        let settings = json!({"nodes": {"x": {"name": name, "defaults": {"http": {"max_content_length": "100mb"}}}}});
        apply_settings(&mut l.nodes, &settings);
        assert_eq!(l.max_content_length(), 100 << 20);
        let flat = json!({"nodes": {"x": {"name": name, "settings": {"http.max_content_length": "20mb"}}}});
        apply_settings(&mut l.nodes, &flat);
        assert_eq!(l.max_content_length(), 20 << 20);
        let rows = json!([{"node_name": name, "name": "search", "size": "4"}, {"node_name": name, "name": "write", "size": 2}]);
        apply_thread_pools(&mut l.nodes, &rows);
        assert_eq!(l.search_pool(), Some(4));
        assert_eq!(l.nodes[0].write_pool, Some(2));
        let b = budget(&l, Mode::Auto, 256 << 20).unwrap();
        assert_eq!(b.bulk_cap_bytes, 19 << 20);
    }

    #[test]
    fn explicit_flags_are_found_before_the_separator() {
        let e = Explicit::from_args(&[
            "searchdump",
            "--limit=5",
            "--maxBulkBytes",
            "8mb",
            "--",
            "--concurrency=2",
        ]);
        assert!(e.limit && e.max_bulk_bytes && !e.concurrency && !e.slices && !e.max_response_mb);
    }

    #[test]
    fn metrics_follow_the_version() {
        let es = |v: &str| ClusterInfo::new(Flavor::Elastic, v);
        assert_eq!(stats_metrics(Some(&es("7.8.1"))), "jvm,breaker");
        assert_eq!(
            stats_metrics(Some(&es("7.10.2"))),
            "jvm,breaker,indexing_pressure"
        );
        assert_eq!(
            stats_metrics(Some(&ClusterInfo::new(Flavor::OpenSearch, "1.3.20"))),
            "jvm,breaker,indexing_pressure"
        );
        // no version known (the probe sends no `GET /`): ask for all, retry without on a 400
        assert_eq!(stats_metrics(None), "jvm,breaker,indexing_pressure");
        assert_eq!(es_bytes("100mb"), Some(100 << 20));
        assert_eq!(es_bytes("1.5gb"), Some(3 << 29));
        assert_eq!(es_bytes("2048"), Some(2048));
        assert_eq!(es_bytes("lots"), None);
    }
}
