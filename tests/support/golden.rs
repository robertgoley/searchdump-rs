//! Record/replay golden infrastructure (TST-INF-003, L-303).
//!
//! **Record** (`IT_RECORD=1`, live cluster): a [`Recorder`] sits between the binary and the
//! cluster (it is a [`RouterMock`] whose handler forwards every request upstream). Each
//! exchange is normalised and written to
//! `tests/golden/<engine>-<version>/<case>/{NNN.req.json,NNN.resp.json,meta.toml}`.
//!
//! **Normalise.** Volatile values become stable placeholders of the same JSON type, so a
//! recording can still be replayed: `took`/`ingest_took`/`creation_time` -> 0; `uuid`,
//! `index_uuid`, `cluster_uuid` -> `"<uuid>"`; `creation_date` -> `"<creation_date>"`; the
//! node `name` in `GET /` -> `"<node>"`; scroll and PIT ids -> `"<scroll:N>"`/`"<pit:N>"`
//! (numbered by first appearance, and replaced wherever the same value reappears: request
//! bodies, paths, query strings); ISM `_seq_no`/`_primary_term` -> 1001, 1002, ... (still
//! numbers, also replaced in `if_seq_no`/`if_primary_term`); ILM `modified_date` -> `"<date>"`,
//! `modified_date_millis`/ISM `last_updated_time`/`enabled_time` -> 0. Only `Content-Type`, `X-Elastic-Product` and `Warning`
//! response headers are kept (`Date`, `X-Opaque-Id` and the rest are dropped); no request
//! header is stored, so credentials never reach a golden file. Non-NDJSON request bodies are
//! stored with sorted keys (the tool's search-body key order is not stable, finding G-1). `_shards` counts are kept:
//! they are deterministic on the single-node test clusters and the tool reads `failed`.
//!
//! **Replay** (offline, T1): a [`Replay`] serves the recorded responses for requests with the
//! same method, normalised path and query (in recorded order per key) and reports every
//! difference in request shape: a body that differs, a request that was never recorded,
//! and recorded requests that were never made.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};

use super::router_mock::{Recorded, Reply, RouterMock};

/// value -> token, and the per-kind counters.
type TokenTable = (HashMap<String, String>, HashMap<&'static str, usize>);

/// Volatile-value normaliser with a stable token table.
#[derive(Default)]
pub struct Normaliser {
    tokens: Mutex<TokenTable>,
    /// ISM `_seq_no`/`_primary_term` (global counters in the ISM config index) -> 1001, 1002, ...
    /// They stay numbers so the tool can echo them back as `if_seq_no`/`if_primary_term`.
    nums: Mutex<HashMap<String, u64>>,
}

const ID_KEYS: &[(&str, &str)] = &[
    ("_scroll_id", "scroll"),
    ("scroll_id", "scroll"),
    ("pit_id", "pit"),
];

impl Normaliser {
    fn token(&self, kind: &'static str, v: &str) -> String {
        if v.starts_with('<') && v.ends_with('>') {
            return v.to_string();
        }
        let mut g = self.tokens.lock().unwrap();
        if let Some(t) = g.0.get(v) {
            return t.clone();
        }
        let n = g.1.entry(kind).or_insert(0);
        *n += 1;
        let t = format!("<{kind}:{n}>");
        g.0.insert(v.to_string(), t.clone());
        t
    }

    fn known(&self, v: &str) -> Option<String> {
        if let Some(t) = self.tokens.lock().unwrap().0.get(v).cloned() {
            return Some(t);
        }
        self.nums.lock().unwrap().get(v).map(|n| n.to_string())
    }

    fn learn_nums(&self, v: &Value) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if (k == "_seq_no" || k == "_primary_term") && x.is_u64() {
                        let mut g = self.nums.lock().unwrap();
                        let next = 1001 + g.len() as u64;
                        g.entry(x.to_string()).or_insert(next);
                    }
                    self.learn_nums(x);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| self.learn_nums(x)),
            _ => {}
        }
    }

    /// Learn scroll/PIT ids from a response body (call before [`Self::apply`]).
    pub fn learn(&self, path: &str, v: &Value) {
        let pit_open = path.ends_with("/_pit") || path.ends_with("/_search/point_in_time");
        if let Value::Object(m) = v
            && pit_open
            && let Some(Value::String(s)) = m.get("id")
        {
            self.token("pit", s);
        }
        self.learn_walk(v, None);
        if path.contains("/_ism/") {
            self.learn_nums(v);
        }
    }

    fn learn_walk(&self, v: &Value, parent: Option<&str>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if let Value::String(s) = x {
                        if let Some((_, kind)) = ID_KEYS.iter().find(|(key, _)| key == k) {
                            self.token(kind, s);
                        } else if k == "id" && parent == Some("pit") {
                            self.token("pit", s);
                        }
                    }
                    self.learn_walk(x, Some(k));
                }
            }
            Value::Array(a) => a.iter().for_each(|x| self.learn_walk(x, parent)),
            _ => {}
        }
    }

    /// Mask volatile keys and replace every known id, recursively.
    pub fn apply(&self, v: &mut Value) {
        self.apply_at(v, true);
    }

    fn apply_at(&self, v: &mut Value, top: bool) {
        match v {
            Value::Object(m) => {
                let is_root_info =
                    top && m.contains_key("cluster_name") && m.contains_key("version");
                for (k, x) in m.iter_mut() {
                    match k.as_str() {
                        "took"
                        | "ingest_took"
                        | "creation_time"
                        | "last_updated_time"
                        | "enabled_time"
                        | "modified_date_millis"
                        | "task_max_waiting_in_queue_millis"
                        | "number_of_pending_tasks"
                        | "number_of_in_flight_fetch"
                            if x.is_number() =>
                        {
                            *x = json!(0)
                        }
                        "modified_date" if x.is_string() => *x = json!("<date>"),
                        // ILM `in_use_by` lists come from hash sets (order varies per run), and
                        // system data streams (`.logs-deprecation.*`) appear lazily: sort, and
                        // drop dot-prefixed indices / data streams
                        "in_use_by" if x.is_object() => {
                            for (lk, l) in x.as_object_mut().unwrap().iter_mut() {
                                if let Value::Array(a) = l {
                                    if lk == "indices" || lk == "data_streams" {
                                        a.retain(|e| {
                                            !e.as_str().is_some_and(|s| s.starts_with('.'))
                                        });
                                    }
                                    a.sort_by_key(|e| e.to_string());
                                }
                            }
                            self.apply_at(x, false)
                        }
                        "_seq_no" | "_primary_term" if x.is_u64() => {
                            if let Some(n) = self.nums.lock().unwrap().get(&x.to_string()) {
                                *x = json!(*n);
                            }
                        }
                        "uuid" | "index_uuid" | "cluster_uuid" if x.is_string() => {
                            *x = json!("<uuid>")
                        }
                        "creation_date" if x.is_string() => *x = json!("<creation_date>"),
                        "name" if is_root_info && x.is_string() => *x = json!("<node>"),
                        _ => self.apply_at(x, false),
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| self.apply_at(x, false)),
            Value::String(s) => {
                if let Some(t) = self.known(s) {
                    *s = t;
                } else if s.contains('/') && s.contains('[') {
                    // error reasons embed index uuids: "index [name/xSJ6dImnRFq_tVidr8iayw] ..."
                    *s = index_uuid_re().replace_all(s, "[$1/<uuid>]").into_owned();
                }
            }
            _ => {}
        }
    }

    fn path(&self, p: &str) -> String {
        p.split('/')
            .map(|seg| self.known(seg).unwrap_or_else(|| seg.to_string()))
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn index_uuid_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\[([^/\]\[]+)/[A-Za-z0-9_-]{22}\]").unwrap())
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Decode a body for the golden file: gunzip, then NDJSON (array of lines), JSON, or text.
pub fn decode_body(raw: &[u8], headers: &[(String, String)], path: &str) -> Value {
    decode(raw, headers, path, true)
}

fn decode(raw: &[u8], headers: &[(String, String)], path: &str, request: bool) -> Value {
    let mut bytes = raw.to_vec();
    if header(headers, "content-encoding").is_some_and(|e| e.contains("gzip")) {
        let mut out = Vec::new();
        if flate2::read::MultiGzDecoder::new(raw)
            .read_to_end(&mut out)
            .is_ok()
        {
            bytes = out;
        }
    }
    if bytes.is_empty() {
        return Value::Null;
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let ndjson = header(headers, "content-type").is_some_and(|c| c.contains("ndjson"))
        || (request && (path.ends_with("/_bulk") || path.ends_with("/_msearch")));
    if ndjson {
        let lines: Vec<Value> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap_or_else(|_| Value::String(l.to_string())))
            .collect();
        return json!({ "ndjson": lines });
    }
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

fn encode_body(v: &Value) -> Vec<u8> {
    match v {
        Value::Null => vec![],
        Value::String(s) => s.clone().into_bytes(),
        Value::Object(m) if m.len() == 1 && m.contains_key("ndjson") => {
            let mut out = Vec::new();
            for l in m["ndjson"].as_array().into_iter().flatten() {
                out.extend(serde_json::to_vec(l).unwrap());
                out.push(b'\n');
            }
            out
        }
        other => serde_json::to_vec(other).unwrap(),
    }
}

/// Normalised request document.
pub fn request_doc(n: &Normaliser, r: &Recorded) -> Value {
    let mut q: Vec<(String, String)> = r
        .query
        .iter()
        .map(|(k, v)| (k.clone(), n.known(v).unwrap_or_else(|| v.clone())))
        .collect();
    q.sort();
    let mut body = decode_body(&r.body, &r.headers, &r.path);
    n.apply(&mut body);
    if !body.get("ndjson").is_some_and(Value::is_array) {
        // Search bodies are built in sonic_rs objects, whose key order is per-process random
        // once mutated (finding G-1); sort keys so the golden file is stable. NDJSON (bulk)
        // lines keep their exact order: document bytes must stay faithful.
        body = sorted(body);
    }
    json!({
        "method": r.method,
        "path": n.path(&r.path),
        "query": q.into_iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "body": body,
    })
}

const KEEP_RESP_HEADERS: &[&str] = &["content-type", "x-elastic-product", "warning"];

/// Normalised response document (learns ids from the body first).
pub fn response_doc(
    n: &Normaliser,
    path: &str,
    status: u16,
    headers: &[(String, String)],
    body: &[u8],
) -> Value {
    let mut b = decode(body, &[], path, false);
    n.learn(path, &b);
    n.apply(&mut b);
    let mut h = Map::new();
    for (k, v) in headers {
        let k = k.to_ascii_lowercase();
        if KEEP_RESP_HEADERS.contains(&k.as_str()) {
            h.insert(k, json!(v));
        }
    }
    json!({ "status": status, "headers": h, "body": b })
}

fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut e: Vec<(String, Value)> = m.into_iter().collect();
            e.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(e.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn key(req: &Value) -> String {
    format!("{} {} {}", req["method"], req["path"], req["query"])
}

// ---------------------------------------------------------------------------------------------
// Recording

/// Recording proxy in front of `upstream` (a base URL; userinfo becomes basic auth upstream,
/// TLS certificates are not verified: test clusters only).
pub struct Recorder {
    mock: RouterMock,
    log: Arc<Mutex<Vec<(usize, Value, Value)>>>,
}

impl Recorder {
    pub async fn start(upstream: &str) -> Recorder {
        let mut base = url::Url::parse(upstream).expect("recorder upstream url");
        let auth = (!base.username().is_empty()).then(|| {
            (
                base.username().to_string(),
                base.password().map(str::to_string),
            )
        });
        let _ = base.set_username("");
        let _ = base.set_password(None);
        let base = base.as_str().trim_end_matches('/').to_string();
        let client = {
            searchdump_rs::http::tls::install_default_provider();
            reqwest::Client::builder()
        }
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
        let norm = Arc::new(Normaliser::default());
        let log = Arc::new(Mutex::new(Vec::new()));
        let seq = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (n2, l2) = (Arc::clone(&norm), Arc::clone(&log));
        let mock = RouterMock::builder()
            .handler(move |r: Recorded| {
                let (client, base, auth, norm, log) = (
                    client.clone(),
                    base.clone(),
                    auth.clone(),
                    Arc::clone(&n2),
                    Arc::clone(&l2),
                );
                let i = seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    let req_doc = request_doc(&norm, &r);
                    let q = if r.query.is_empty() {
                        String::new()
                    } else {
                        let pairs: Vec<String> = r
                            .query
                            .iter()
                            .map(|(k, v)| {
                                if v.is_empty() {
                                    k.clone()
                                } else {
                                    format!("{k}={v}")
                                }
                            })
                            .collect();
                        format!("?{}", pairs.join("&"))
                    };
                    let m = reqwest::Method::from_bytes(r.method.as_bytes()).unwrap();
                    let mut rb = client.request(m, format!("{base}{}{q}", r.path));
                    for (k, v) in &r.headers {
                        let lk = k.to_ascii_lowercase();
                        if ![
                            "host",
                            "content-length",
                            "connection",
                            "transfer-encoding",
                            "authorization",
                            "accept-encoding",
                        ]
                        .contains(&lk.as_str())
                        {
                            rb = rb.header(k, v);
                        }
                    }
                    if let Some((u, p)) = &auth {
                        rb = rb.basic_auth(u, p.as_deref());
                    } else if let Some(a) = r.header("authorization") {
                        rb = rb.header("authorization", a);
                    }
                    let resp = rb.body(r.body.clone()).send().await;
                    let (status, headers, body) = match resp {
                        Ok(resp) => {
                            let status = resp.status().as_u16();
                            let headers: Vec<(String, String)> = resp
                                .headers()
                                .iter()
                                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                                .collect();
                            (
                                status,
                                headers,
                                resp.bytes().await.unwrap_or_default().to_vec(),
                            )
                        }
                        Err(e) => (
                            502,
                            vec![],
                            format!("{{\"error\":\"recorder: {e}\"}}").into_bytes(),
                        ),
                    };
                    let resp_doc = response_doc(&norm, &r.path, status, &headers, &body);
                    log.lock().unwrap().push((i, req_doc, resp_doc));
                    let mut reply = Reply::json(status, "");
                    reply.headers.clear();
                    for (k, v) in &headers {
                        if KEEP_RESP_HEADERS.contains(&k.to_ascii_lowercase().as_str()) {
                            reply.headers.push((k.clone(), v.clone()));
                        }
                    }
                    reply.body = body;
                    reply
                }
            })
            .start()
            .await;
        Recorder { mock, log }
    }

    pub fn url(&self) -> String {
        self.mock.url()
    }

    /// Exchanges in request-arrival order.
    pub fn exchanges(&self) -> Vec<(Value, Value)> {
        let mut l = self.log.lock().unwrap().clone();
        l.sort_by_key(|(i, _, _)| *i);
        l.into_iter().map(|(_, a, b)| (a, b)).collect()
    }

    /// Replace the case's exchange files in `dir` (meta.toml and inputs are left alone).
    pub fn save(&self, dir: &Path) -> std::io::Result<usize> {
        std::fs::create_dir_all(dir)?;
        for e in std::fs::read_dir(dir)? {
            let p = e?.path();
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".req.json") || name.ends_with(".resp.json") {
                std::fs::remove_file(p)?;
            }
        }
        let ex = self.exchanges();
        for (i, (req, resp)) in ex.iter().enumerate() {
            write_pretty(&dir.join(format!("{:03}.req.json", i + 1)), req)?;
            write_pretty(&dir.join(format!("{:03}.resp.json", i + 1)), resp)?;
        }
        Ok(ex.len())
    }
}

fn write_pretty(p: &Path, v: &Value) -> std::io::Result<()> {
    let mut s = serde_json::to_string_pretty(v).unwrap();
    s.push('\n');
    std::fs::write(p, s)
}

// ---------------------------------------------------------------------------------------------
// Case metadata (`meta.toml`: flat `key = <TOML value>`; strings and string arrays are written
// in the JSON-compatible subset of TOML so they read back with serde_json)

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub case: String,
    pub engine: String,
    pub version: String,
    pub image: String,
    /// Tool arguments; `{{URL}}` = server base URL, `{{OUT}}` = output file, `{{DIR}}` = case dir.
    pub args: Vec<String>,
    pub exit_code: i32,
    /// `expected.out` holds the output file of the recording run.
    pub has_output: bool,
}

impl Meta {
    pub fn write(&self, dir: &Path) -> std::io::Result<()> {
        let s = |v: &str| serde_json::to_string(v).unwrap();
        let text = format!(
            "# golden case (TST-INF-003); regenerate with scripts/golden/record.sh\n\
             case = {}\nengine = {}\nversion = {}\nimage = {}\nargs = {}\nexit_code = {}\nhas_output = {}\n",
            s(&self.case),
            s(&self.engine),
            s(&self.version),
            s(&self.image),
            serde_json::to_string(&self.args).unwrap(),
            self.exit_code,
            self.has_output
        );
        std::fs::write(dir.join("meta.toml"), text)
    }

    pub fn read(dir: &Path) -> Meta {
        let text = std::fs::read_to_string(dir.join("meta.toml")).expect("meta.toml");
        let mut kv = HashMap::new();
        for l in text.lines() {
            if let Some((k, v)) = l.split_once(" = ")
                && !k.starts_with('#')
            {
                kv.insert(
                    k.trim().to_string(),
                    serde_json::from_str::<Value>(v.trim()).expect(l),
                );
            }
        }
        let st = |k: &str| {
            kv.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        Meta {
            case: st("case"),
            engine: st("engine"),
            version: st("version"),
            image: st("image"),
            args: kv
                .get("args")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            exit_code: kv.get("exit_code").and_then(Value::as_i64).unwrap_or(0) as i32,
            has_output: kv
                .get("has_output")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub fn expand(&self, url: &str, out: &Path, dir: &Path) -> Vec<String> {
        self.args
            .iter()
            .map(|a| {
                a.replace("{{URL}}", url)
                    .replace("{{OUT}}", out.to_str().unwrap())
                    .replace("{{DIR}}", dir.to_str().unwrap())
            })
            .collect()
    }
}

/// Root of the committed goldens (`IT_GOLDEN_DIR` overrides, e.g. for a re-record check).
pub fn golden_root() -> PathBuf {
    std::env::var("IT_GOLDEN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden"))
}

/// Every case directory (`<root>/<engine>-<version>/<case>/meta.toml`).
pub fn cases(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for ev in std::fs::read_dir(root).into_iter().flatten().flatten() {
        for c in std::fs::read_dir(ev.path()).into_iter().flatten().flatten() {
            if c.path().join("meta.toml").is_file() {
                out.push(c.path());
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// Replay

struct ReplayState {
    exchanges: Vec<(Value, Value)>,
    used: Vec<bool>,
    problems: Vec<String>,
}

/// Router-mock replay of one recorded case.
pub struct Replay {
    mock: RouterMock,
    state: Arc<Mutex<ReplayState>>,
}

fn load_json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

impl Replay {
    pub async fn start(dir: &Path) -> Replay {
        let mut exchanges = vec![];
        for i in 1.. {
            let req = dir.join(format!("{i:03}.req.json"));
            if !req.is_file() {
                break;
            }
            exchanges.push((
                load_json(&req),
                load_json(&dir.join(format!("{i:03}.resp.json"))),
            ));
        }
        let n = exchanges.len();
        let state = Arc::new(Mutex::new(ReplayState {
            exchanges,
            used: vec![false; n],
            problems: vec![],
        }));
        let st = Arc::clone(&state);
        let norm = Arc::new(Normaliser::default());
        let mock = RouterMock::builder()
            .handler(move |r: Recorded| {
                let st = Arc::clone(&st);
                let norm = Arc::clone(&norm);
                async move {
                    let actual = request_doc(&norm, &r);
                    let k = key(&actual);
                    let mut s = st.lock().unwrap();
                    let found =
                        (0..s.exchanges.len()).find(|&i| !s.used[i] && key(&s.exchanges[i].0) == k);
                    let Some(i) = found else {
                        s.problems
                            .push(format!("unrecorded request: {k} body={}", actual["body"]));
                        return Reply::json(
                            599,
                            r#"{"error":"golden replay: no recorded exchange"}"#,
                        );
                    };
                    s.used[i] = true;
                    let (want, resp) = s.exchanges[i].clone();
                    if want["body"] != actual["body"] {
                        s.problems.push(format!(
                            "{:03}.req.json body differs ({k}):\n  recorded: {}\n  actual:   {}",
                            i + 1,
                            want["body"],
                            actual["body"]
                        ));
                    }
                    let mut reply = Reply::json(resp["status"].as_u64().unwrap_or(200) as u16, "");
                    reply.headers.clear();
                    if let Some(h) = resp["headers"].as_object() {
                        for (k, v) in h {
                            reply
                                .headers
                                .push((k.clone(), v.as_str().unwrap_or("").to_string()));
                        }
                    }
                    reply.body = encode_body(&resp["body"]);
                    reply
                }
            })
            .start()
            .await;
        Replay { mock, state }
    }

    pub fn url(&self) -> String {
        self.mock.url()
    }

    /// Request-shape differences, plus recorded requests that were never made.
    pub fn problems(&self) -> Vec<String> {
        let s = self.state.lock().unwrap();
        let mut p = s.problems.clone();
        for (i, used) in s.used.iter().enumerate() {
            if !used {
                p.push(format!(
                    "{:03}.req.json never requested: {}",
                    i + 1,
                    key(&s.exchanges[i].0)
                ));
            }
        }
        p
    }
}
