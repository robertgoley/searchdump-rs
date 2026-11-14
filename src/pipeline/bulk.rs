//! Bulk writer: turns document lines (search hits or plain documents) into
//! `_bulk` requests.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use bytes::Bytes;
use reqwest::Method;
use sonic_rs::{JsonValueTrait, LazyValue};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::http::{Body, ClusterInfo, EsClient, RetryMode, RetryPolicy, send_with_retry};

/// A per-document error that must stop the run (it would apply to every
/// document alike), never a per-line rejection.
#[derive(Debug)]
pub struct FatalDoc(pub String);

impl std::fmt::Display for FatalDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FatalDoc {}

/// The data stream behind a backing index name: `.ds-<stream>-<yyyy.MM.dd>-<gen>`
/// (7.11+, OpenSearch) or `.ds-<stream>-<gen>` (7.9/7.10) (TST-BRK-XV-003).
pub fn data_stream_of(index: &str) -> Option<&str> {
    let rest = index.strip_prefix(".ds-")?;
    let (head, generation) = rest.rsplit_once('-')?;
    if generation.len() != 6 || !generation.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let is_date = |d: &str| {
        d.len() == 10
            && d.bytes().enumerate().all(|(i, b)| {
                if i == 4 || i == 7 {
                    b == b'.'
                } else {
                    b.is_ascii_digit()
                }
            })
    };
    let stream = match head.rsplit_once('-') {
        Some((stream, date)) if is_date(date) => stream,
        _ => head,
    };
    (!stream.is_empty()).then_some(stream)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkAction {
    Index,
    Create,
    Update,
    Delete,
}

impl BulkAction {
    pub fn parse(raw: &str) -> Result<Self> {
        Ok(match raw {
            "index" => Self::Index,
            "create" => Self::Create,
            "update" => Self::Update,
            "delete" => Self::Delete,
            other => bail!("invalid --bulkAction '{other}' (index, create, update, delete)"),
        })
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BulkOptions {
    pub info: ClusterInfo,
    /// `--output-index` / URL index; `None` = use each document's `_index`.
    pub index: Option<String>,
    pub doc_type: Option<String>,
    pub prefix: String,
    pub suffix: String,
    /// `--renamePrefix`/`--renameIndex` (includes prefix/suffix); used when
    /// documents keep their own `_index`.
    pub rename: Option<std::sync::Arc<crate::rename::IndexRenamer>>,
    pub action: BulkAction,
    pub handle_version: bool,
    pub version_type: Option<String>,
    /// Meta fields copied into the action line (bare names).
    pub meta_fields: Vec<String>,
    pub refresh: bool,
    pub ignore_es_write_errors: bool,
    pub ignore_errors: bool,
    pub max_bulk_bytes: usize,
    pub retry: RetryPolicy,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WriteReport {
    pub written: u64,
    pub failed: u64,
    /// Input lines refused before encoding because they were not exactly one
    /// JSON object (INJ-F1). Also counted in `failed`; any rejection makes
    /// the run exit non-zero.
    pub rejected: u64,
    /// Written documents that replaced an existing one with the same `_id` (item status 200
    /// for `index`; TST-BRK-IN-010). Included in `written`.
    pub overwritten: u64,
    /// L-804b: failed items refused by a `cluster_block_exception`. Included in `failed`.
    pub blocked: u64,
    /// L-804b: of `blocked`, those from the read-only-allow-delete block that the disk
    /// flood-stage watermark sets.
    pub disk_blocked: u64,
}

/// The read-only-allow-delete block that the disk flood-stage watermark sets (its reason
/// text varies by version: `FORBIDDEN/12/index read-only / allow delete (api)`, `disk usage
/// exceeded flood-stage watermark, index has read-only-allow-delete block`).
pub(crate) fn is_disk_block(item: &str) -> bool {
    [
        "allow delete",
        "allow-delete",
        "allow_delete",
        "flood-stage",
        "flood stage",
    ]
    .iter()
    .any(|m| item.contains(m))
}

impl WriteReport {
    fn add(&mut self, other: WriteReport) {
        self.overwritten += other.overwritten;
        self.written += other.written;
        self.failed += other.failed;
        self.rejected += other.rejected;
        self.blocked += other.blocked;
        self.disk_blocked += other.disk_blocked;
    }
}

/// `_bulk` writer. With `--concurrency N > 1` each batch is written by a
/// worker task (a clone sharing client, options and the refresh lock); at most
/// N batches are in flight and [`BulkSink::drain`] collects their reports.
pub struct BulkSink {
    client: EsClient,
    opts: Arc<BulkOptions>,
    cancel: CancellationToken,
    pub report: WriteReport,
    warned_ttl: Arc<AtomicBool>,
    /// L-804b: a bulk request holding documents without `_id` reached the cluster (shared
    /// by the workers), so a rerun could index them twice.
    auto_id_written: Arc<AtomicBool>,
    concurrency: usize,
    throttle: Option<Duration>,
    inflight: JoinSet<Result<WriteReport>>,
    /// At most one `_refresh` in flight (others are skipped).
    refresh_lock: Arc<tokio::sync::Mutex<()>>,
    /// 1-based number (in the input) of the next document handed to `write_batch`, so a
    /// rejected line can be located (TST-BRK-IN-005).
    next_doc: u64,
    /// When the first batch was handed over (TST-BRK-SRV-008 re-creation check).
    first_batch: Option<std::time::Instant>,
    /// uuid of the single target index, noted once mid-run (TST-BRK-SRV-008).
    target_uuid: Option<String>,
    uuid_checked: bool,
    /// The single target index was missing (404) when its uuid was to be noted, after
    /// [`UUID_CAPTURE_AFTER`] of writes: it was deleted mid-run (TST-BRK-SRV-008).
    target_gone_mid_run: bool,
    /// The 409 explanation is printed once per run (TST-BRK-SRV-011).
    conflict_hint_shown: bool,
    data_stream_hint_shown: bool,
}

fn get<'a>(line: &'a [u8], path: &[&str]) -> Option<LazyValue<'a>> {
    sonic_rs::get(line, path).ok().filter(|v| !v.is_null())
}

fn push_json_str(out: &mut Vec<u8>, s: &str) {
    sonic_rs::to_writer(&mut *out, s).expect("string serialization");
}

/// The top-level members of one document line, parsed in a single pass
/// (a hit has a handful of keys; looking each one up with `sonic_rs::get`
/// rescanned the whole line per lookup).
pub(crate) struct Top<'a> {
    members: Vec<(std::borrow::Cow<'a, str>, LazyValue<'a>)>,
}

impl<'a> Top<'a> {
    /// All or nothing: any parse error rejects the whole line, so a
    /// truncated fragment never yields the members read before the error
    /// (INJ-F1, TST-INJ-003).
    fn parse(line: &'a [u8]) -> Result<Self> {
        let mut members = Vec::new();
        for m in sonic_rs::to_object_iter(line) {
            let (k, v) = m.map_err(|e| anyhow!("malformed document line: {e}"))?;
            if !v.is_null() {
                members.push((k, v));
            }
        }
        Ok(Self { members })
    }

    /// L-808o: one checked pass that both validates the line and collects its members, for
    /// the hot path of [`check_and_parse`]. `Some` only when the checked sonic-rs iterator
    /// accepted every member (it validates each value and the UTF-8) *and* only `[ \t]* }
    /// [ \t]*` follows the last value, i.e. the line is exactly one JSON object as the full
    /// `from_slice` check in [`check_doc_line`] requires. Any doubt returns `None` and the
    /// caller takes the old two-pass path, so verdicts and messages stay the same.
    fn parse_strict(line: &'a [u8]) -> Option<Self> {
        let base = line.as_ptr() as usize;
        let mut end = memchr::memchr(b'{', line)? + 1;
        let mut members = Vec::new();
        for m in sonic_rs::to_object_iter(line) {
            let (k, v) = m.ok()?;
            let raw = v.as_raw_str();
            let off = (raw.as_ptr() as usize).checked_sub(base)?;
            let stop = off.checked_add(raw.len()).filter(|&e| e <= line.len())?;
            end = end.max(stop);
            if !v.is_null() {
                members.push((k, v));
            }
        }
        let mut tail = line[end..].iter().skip_while(|b| matches!(b, b' ' | b'\t'));
        if tail.next() != Some(&b'}') || !tail.all(|b| matches!(b, b' ' | b'\t')) {
            return None;
        }
        Some(Self { members })
    }

    /// The first top-level key that appears more than once. Rust would use
    /// the first value and Node's `JSON.parse` the last, so such a document
    /// is ambiguous and rejected (H-INJ-1, TST-INJ-008; Q13 default).
    fn duplicate_key(&self) -> Option<&str> {
        self.members.iter().enumerate().find_map(|(i, (k, _))| {
            self.members[..i]
                .iter()
                .any(|(p, _)| p == k)
                .then_some(k.as_ref())
        })
    }

    fn get(&self, key: &str) -> Option<&LazyValue<'a>> {
        self.members.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str().map(str::to_string))
    }
}

/// A bulk input line must be exactly one complete JSON object: no truncated
/// or trailing fragment and no other value type (INJ-F1, TST-INJ-003).
/// The non-empty `\n`-separated lines of `buf`, found with SIMD `memchr` (L-808n). Same
/// result as `buf.split(|b| *b == b'\n').filter(|l| !l.is_empty())`, whose byte-by-byte
/// predicate was a measurable share of restore CPU.
pub(crate) fn nonempty_lines(buf: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut start = 0usize;
    memchr::memchr_iter(b'\n', buf)
        .map(Some)
        .chain(std::iter::once(None))
        .filter_map(move |nl| {
            let end = nl.unwrap_or(buf.len());
            let line = &buf[start.min(end)..end];
            start = end + 1;
            (!line.is_empty()).then_some(line)
        })
}

#[cfg_attr(not(any(feature = "fuzz", test)), allow(dead_code))]
pub(crate) fn check_doc_line(line: &[u8]) -> std::result::Result<(), String> {
    precheck_doc_line(line)?;
    sonic_rs::from_slice::<serde::de::IgnoredAny>(line)
        .map_err(|e| format!("not exactly one JSON object: {e}"))?;
    check_escapes(line)
}

/// Why [`check_and_parse`] refused a line: `Line` is the [`check_doc_line`] reason,
/// `Parse` the error [`BulkSink::encode_doc`] would have returned for it.
pub(crate) enum CheckErr {
    Line(String),
    Parse(anyhow::Error),
}

/// [`check_doc_line`] followed by the member parse of [`BulkSink::encode_doc`], with the
/// line walked once instead of twice on the common path (L-808o). Same verdicts and
/// messages as the two calls in sequence: a line the single pass is not sure about goes
/// through exactly those two calls.
pub(crate) fn check_and_parse(line: &[u8]) -> std::result::Result<Top<'_>, CheckErr> {
    precheck_doc_line(line).map_err(CheckErr::Line)?;
    if let Some(top) = Top::parse_strict(line) {
        check_escapes(line).map_err(CheckErr::Line)?;
        return Ok(top);
    }
    sonic_rs::from_slice::<serde::de::IgnoredAny>(line)
        .map_err(|e| CheckErr::Line(format!("not exactly one JSON object: {e}")))?;
    check_escapes(line).map_err(CheckErr::Line)?;
    Top::parse(line).map_err(CheckErr::Parse)
}

/// The cheap checks of [`check_doc_line`] that run before any parse.
fn precheck_doc_line(line: &[u8]) -> std::result::Result<(), String> {
    if line.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err("not a JSON object".to_string());
    }
    if memchr::memchr2(b'\n', b'\r', line).is_some() {
        return Err("contains a line break".to_string());
    }
    // before any recursive parse: a 1k+ deep document overflowed the stack (TST-BRK-IN-008)
    if let Some(at) = too_deep(line, MAX_DOC_DEPTH) {
        return Err(format!(
            "nested more than {MAX_DOC_DEPTH} levels deep (at byte {at}); Elasticsearch and OpenSearch refuse such documents too"
        ));
    }
    Ok(())
}

/// Deepest `[`/`{` nesting a document line may have (TST-BRK-IN-008). Elasticsearch and
/// OpenSearch stop at 1000 levels (object fields at index.mapping.depth.limit, 20); the
/// recursive parses need the large stacks set in main.rs.
pub(crate) const MAX_DOC_DEPTH: usize = 1000;

/// Byte offset where `line` first nests deeper than `max`, scanning linearly (strings
/// skipped), so the check itself cannot overflow the stack.
pub(crate) fn too_deep(line: &[u8], max: usize) -> Option<usize> {
    // L-808n: the depth can never exceed the number of `[`/`{` bytes (inside strings too),
    // so a line with at most `max` of them needs no byte-by-byte scan (SIMD count instead).
    memchr::memchr2_iter(b'[', b'{', line).nth(max)?;
    let (mut depth, mut in_str, mut esc) = (0usize, false, false);
    for (i, &b) in line.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if b == b'\\' {
                esc = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > max {
                    return Some(i);
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// sonic-rs skips string *values* without decoding them, so an invalid
/// escape such as `"\u%%%%"` passed the check above and then failed the
/// whole bulk request on the cluster (L-505 fuzz finding). Backslashes only
/// occur inside strings in a line that parsed, so a linear scan is exact.
pub(crate) fn check_escapes(line: &[u8]) -> std::result::Result<(), String> {
    let mut i = 0;
    while let Some(off) = memchr::memchr(b'\\', &line[i..]) {
        let at = i + off;
        let ok = match line.get(at + 1) {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                i = at + 2;
                true
            }
            Some(b'u') => {
                i = at + 6;
                line.get(at + 2..at + 6)
                    .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit))
            }
            _ => false,
        };
        if !ok {
            return Err(format!(
                "not exactly one JSON object: invalid escape at byte {}",
                at + 1
            ));
        }
    }
    Ok(())
}

/// `_id`, `routing` and `parent` must be JSON strings; numbers are converted
/// to strings. Anything else (objects, arrays, booleans) is rejected so one
/// bad document cannot fail the whole bulk request (INJ-F3, TST-INJ-005).
fn push_id_like(out: &mut Vec<u8>, name: &str, v: &LazyValue) -> Result<()> {
    push_id_like_raw(out, name, v.as_raw_str())
}

/// [`push_id_like`] for an already validated raw JSON value. L-629a: a string is sent as
/// given and a number literal becomes a string of exactly its text (`-0` -> "-0", `1e3` ->
/// "1e3", `0.0` -> "0.0"): ids are kept byte for byte, never parsed as numbers (this
/// replaces L-629's Node-style `Number#toString`, which turned `-0` into "0").
fn push_id_like_raw(out: &mut Vec<u8>, name: &str, raw: &str) -> Result<()> {
    match raw.as_bytes().first() {
        Some(b'"') => out.extend_from_slice(raw.as_bytes()),
        Some(b'-' | b'0'..=b'9') => {
            if !is_json_number(raw) {
                bail!("{name} is not a valid JSON number: {}", short(raw));
            }
            push_json_str(out, raw)
        }
        _ => bail!("{name} must be a string or a number, got {}", short(raw)),
    }
    Ok(())
}

/// RFC 8259 number grammar: `-? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)?`.
fn is_json_number(raw: &str) -> bool {
    fn digits(b: &[u8], i: &mut usize) -> usize {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i - start
    }
    let b = raw.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i) {
        Some(b'0') => i += 1,
        Some(b'1'..=b'9') => {
            digits(b, &mut i);
        }
        _ => return false,
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(b, &mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(b, &mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// `version` must be an integer within u64 (INJ-F3).
fn checked_version(raw: &str) -> Result<u64> {
    raw.parse::<u64>().map_err(|_| {
        anyhow!(
            "version must be an integer between 0 and 2^64-1, got {}",
            short(raw)
        )
    })
}

fn short(raw: &str) -> String {
    preview(raw.as_bytes())
}

/// A short, single-line preview of a rejected line for the error log.
fn preview(line: &[u8]) -> String {
    const MAX: usize = 120;
    let cut = &line[..line.len().min(MAX)];
    let mut s: String = String::from_utf8_lossy(cut)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if line.len() > MAX {
        s.push_str("...");
    }
    s
}

/// Meta field lookup in elasticdump order: `X`, `_X`, `fields.X`, `fields._X`
/// (single-element arrays from `fields` are unwrapped). Returns raw JSON.
fn meta(top: &Top, name: &str) -> Option<String> {
    let under = format!("_{name}");
    if let Some(v) = top.get(name).or_else(|| top.get(&under)) {
        return Some(v.as_raw_str().to_string());
    }
    let fields = top.get("fields")?.as_raw_str().as_bytes();
    let v = get(fields, &[name]).or_else(|| get(fields, &[under.as_str()]))?;
    if !v.is_array() {
        return Some(v.as_raw_str().to_string());
    }
    let elems: Vec<LazyValue> = sonic_rs::to_array_iter(v.as_raw_str().as_bytes())
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    match elems.as_slice() {
        [one] => Some(one.as_raw_str().to_string()),
        _ => None,
    }
}

impl BulkSink {
    #[cfg(any(feature = "fuzz", test))]
    pub fn fuzz_client(&self) -> &EsClient {
        &self.client
    }

    pub fn new(client: EsClient, opts: BulkOptions, cancel: CancellationToken) -> Self {
        Self {
            client,
            opts: Arc::new(opts),
            cancel,
            report: WriteReport::default(),
            warned_ttl: Arc::new(AtomicBool::new(false)),
            auto_id_written: Arc::new(AtomicBool::new(false)),
            concurrency: 1,
            throttle: None,
            inflight: JoinSet::new(),
            refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            next_doc: 1,
            first_batch: None,
            target_uuid: None,
            uuid_checked: false,
            target_gone_mid_run: false,
            conflict_hint_shown: false,
            data_stream_hint_shown: false,
        }
    }

    /// The target cluster.
    pub fn target_info(&self) -> &ClusterInfo {
        &self.opts.info
    }

    /// The single target index (see [`Self::single_target`]) when it does not exist yet,
    /// so the first bulk request would create it with dynamic mappings (L-655).
    pub async fn missing_target(&self) -> Option<String> {
        let name = self.single_target()?;
        self.target_uuid(&name).await.is_err().then_some(name)
    }

    /// The one concrete index every document goes to, when the output names one (no
    /// rename, no wildcard or list); `None` otherwise.
    fn single_target(&self) -> Option<String> {
        if self.opts.rename.is_some() {
            return None;
        }
        let i = self.opts.index.as_deref()?;
        let name = format!("{}{}{}", self.opts.prefix, i, self.opts.suffix);
        (!name.contains(['*', ',', '?'])).then_some(name)
    }

    /// `index.uuid` of `name` when it resolves to one index; `Err(())` on a 404.
    async fn target_uuid(&self, name: &str) -> std::result::Result<Option<String>, ()> {
        let Ok(r) = self
            .client
            .send(
                Method::GET,
                &[name, "_settings"],
                &[("filter_path", "*.settings.index.uuid")],
                false,
                Body::Empty,
            )
            .await
        else {
            return Ok(None);
        };
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(());
        }
        if !r.status().is_success() {
            return Ok(None);
        }
        let Ok(b) = r.bytes().await else {
            return Ok(None);
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) else {
            return Ok(None);
        };
        Ok(v.as_object()
            .filter(|o| o.len() == 1)
            .and_then(|o| o.values().next())
            .and_then(|i| i.pointer("/settings/index/uuid"))
            .and_then(|u| u.as_str())
            .map(str::to_string))
    }

    /// TST-BRK-SRV-008: in a restore that has run for [`UUID_CAPTURE_AFTER`] (short runs keep
    /// Node's request shape), note the single target index's uuid once.
    async fn note_target_uuid(&mut self) {
        let now = std::time::Instant::now();
        let first = *self.first_batch.get_or_insert(now);
        if self.uuid_checked || now.duration_since(first) < UUID_CAPTURE_AFTER {
            return;
        }
        self.uuid_checked = true;
        if let Some(name) = self.single_target() {
            match self.target_uuid(&name).await {
                Ok(u) => self.target_uuid = u,
                // Documents have been written for UUID_CAPTURE_AFTER, so the index existed
                // (or was auto-created); a 404 now means it was deleted mid-run.
                Err(()) => self.target_gone_mid_run = true,
            }
        }
    }

    /// TST-BRK-SRV-008: at the end, the target must still be the index noted mid-run. A 404
    /// means it was deleted; another uuid means it was deleted and re-created (auto-create,
    /// dynamic mappings) while documents were written.
    async fn check_target_index(&mut self) -> Result<()> {
        if self.target_uuid.is_none() && !self.target_gone_mid_run {
            return Ok(());
        }
        let Some(name) = self.single_target() else {
            return Ok(());
        };
        let end = self.target_uuid(&name).await;
        let Some(recreated) =
            target_index_outcome(self.target_uuid.as_deref(), self.target_gone_mid_run, end)
        else {
            return Ok(());
        };
        Err(anyhow!(
            "the target index '{name}' was deleted during the restore{}: documents written before that are gone{}. Re-create the index (with its mappings) and run the restore again",
            if recreated { " and re-created" } else { "" },
            if recreated {
                ", and an automatically re-created index has dynamic mappings"
            } else {
                ""
            }
        ))
    }

    /// The input number of the next document (when `--offset` skipped some).
    pub fn set_next_doc(&mut self, n: u64) {
        self.next_doc = n;
    }

    /// `--concurrency` (max in-flight write batches) and `--throttleInterval`
    /// (pause after each batch is handed off).
    pub fn with_concurrency(mut self, concurrency: usize, throttle: Option<Duration>) -> Self {
        self.concurrency = concurrency.max(1);
        self.throttle = throttle.filter(|d| !d.is_zero());
        self
    }

    fn worker(&self) -> BulkSink {
        BulkSink {
            client: self.client.clone(),
            opts: Arc::clone(&self.opts),
            cancel: self.cancel.clone(),
            report: WriteReport::default(),
            warned_ttl: Arc::clone(&self.warned_ttl),
            auto_id_written: Arc::clone(&self.auto_id_written),
            concurrency: 1,
            throttle: None,
            inflight: JoinSet::new(),
            refresh_lock: Arc::clone(&self.refresh_lock),
            next_doc: 1,
            first_batch: None,
            target_uuid: None,
            uuid_checked: false,
            target_gone_mid_run: false,
            conflict_hint_shown: false,
            data_stream_hint_shown: false,
        }
    }

    fn merge(
        &mut self,
        joined: std::result::Result<Result<WriteReport>, tokio::task::JoinError>,
    ) -> Result<()> {
        match joined {
            Ok(Ok(r)) => {
                self.report.add(r);
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(e) if e.is_cancelled() => Ok(()),
            Err(e) => Err(anyhow!("bulk worker panicked: {e}")),
        }
    }

    /// Hand one batch to the writer. Sequential with `--concurrency 1`.
    pub async fn write_batch(&mut self, data: Bytes) -> Result<()> {
        self.note_target_uuid().await;
        let first = self.next_doc;
        self.next_doc += nonempty_lines(&data).count() as u64;
        if self.concurrency <= 1 {
            self.process_batch(&data, first).await?;
        } else {
            while self.inflight.len() >= self.concurrency {
                let joined = self.inflight.join_next().await.expect("non-empty");
                if let Err(e) = self.merge(joined) {
                    self.inflight.abort_all();
                    return Err(e);
                }
            }
            let mut worker = self.worker();
            // L-808r: the shared batch moves into the worker; it used to be copied here
            self.inflight.spawn(async move {
                worker.process_batch(&data, first).await?;
                Ok(worker.report)
            });
        }
        if let Some(d) = self.throttle {
            tokio::time::sleep(d).await;
        }
        Ok(())
    }

    /// Wait for every in-flight batch; the first error wins.
    pub async fn drain(&mut self) -> Result<()> {
        let concurrent = self.concurrency > 1;
        while let Some(joined) = self.inflight.join_next().await {
            if let Err(e) = self.merge(joined) {
                self.inflight.abort_all();
                return Err(e);
            }
        }
        if concurrent && self.opts.refresh {
            // A per-batch refresh may have been skipped while another ran.
            let _g = self.refresh_lock.lock().await;
            self.refresh().await;
        }
        self.check_target_index().await
    }

    pub fn abort(&mut self) {
        self.inflight.abort_all();
    }

    fn index_for(&self, top: &Top) -> Result<String> {
        let mut base = match &self.opts.index {
            Some(i) => i.clone(),
            None => top.str("_index").ok_or_else(|| {
                anyhow!("document has no _index and no output index was given (--output-index)")
            })?,
        };
        if self.opts.index.is_none()
            && let Some(stream) = data_stream_of(&base)
        {
            match self.opts.action {
                // update/delete must address the backing index itself
                BulkAction::Update | BulkAction::Delete => {}
                BulkAction::Create => base = stream.to_string(),
                BulkAction::Index => {
                    return Err(anyhow::Error::new(FatalDoc(format!(
                        "document from '{base}', a backing index of data stream '{stream}': data streams accept only --bulkAction create (written to the stream '{stream}', which picks the backing index); with index it would land in a plain index named '{base}' instead of the stream"
                    ))));
                }
            }
        }
        if self.opts.index.is_none()
            && let Some(r) = &self.opts.rename
        {
            return r.index(&base);
        }
        let out = format!("{}{}{}", self.opts.prefix, base, self.opts.suffix);
        if self.opts.index.is_none() {
            // `_index` comes from the document: never let `*`, `_all`,
            // `a,b`, `.` or `..` reach a bulk action (INJ-F2).
            crate::rename::validate_index_name(&out)
                .map_err(|e| anyhow!("invalid _index '{base}' in a document: {e}"))?;
        }
        Ok(out)
    }

    /// Append the action (and body) lines for one document.
    pub fn encode_doc(&mut self, line: &[u8], out: &mut Vec<u8>) -> Result<()> {
        let top = Top::parse(line)?;
        self.encode_top(line, &top, out)
    }

    /// [`BulkSink::encode_doc`] for a line whose members are already parsed.
    fn encode_top(&mut self, line: &[u8], top: &Top<'_>, out: &mut Vec<u8>) -> Result<()> {
        let action = self.opts.action;
        if let Some(k) = top.duplicate_key() {
            bail!("duplicate key {k:?} in the document (ambiguous; rejected)");
        }
        let source = top.get("_source");
        if let Some(src) = source {
            // Exactly one JSON object (TST-INJ-009).
            if !src.is_object() {
                bail!(
                    "_source must be a JSON object, got {}",
                    short(src.as_raw_str())
                );
            }
        }
        let is_hit = source.is_some() || top.get("_id").is_some();

        out.extend_from_slice(b"{\"");
        out.extend_from_slice(action.as_str().as_bytes());
        out.extend_from_slice(b"\":{\"_index\":");
        push_json_str(out, &self.index_for(top)?);
        if is_hit && let Some(id) = top.get("_id") {
            out.extend_from_slice(b",\"_id\":");
            push_id_like(out, "_id", id)?;
        }
        if self.opts.info.has_types() {
            let ty = self
                .opts
                .doc_type
                .clone()
                .or_else(|| top.str("_type"))
                .unwrap_or_else(|| "_doc".to_string());
            out.extend_from_slice(b",\"_type\":");
            push_json_str(out, &ty);
        }
        if is_hit {
            for name in self.opts.meta_fields.iter() {
                if matches!(name.as_str(), "timestamp" | "ttl")
                    && !self.opts.info.supports_ttl_meta()
                {
                    if meta(top, name).is_some() && !self.warned_ttl.swap(true, Ordering::Relaxed) {
                        log::warn!("Dropping _{name}: not supported by the target cluster");
                    }
                    continue;
                }
                if let Some(v) = meta(top, name) {
                    out.push(b',');
                    push_json_str(out, name);
                    out.push(b':');
                    if matches!(name.as_str(), "routing" | "parent") {
                        push_id_like_raw(out, name, &v)?;
                    } else {
                        out.extend_from_slice(v.as_bytes());
                    }
                }
            }
            if self.opts.handle_version
                && let Some(v) = top.get("_version").or_else(|| top.get("version"))
            {
                let version = checked_version(v.as_raw_str())?;
                out.extend_from_slice(b",\"version\":");
                out.extend_from_slice(version.to_string().as_bytes());
                if let Some(vt) = &self.opts.version_type {
                    out.extend_from_slice(b",\"version_type\":");
                    push_json_str(out, vt);
                }
            }
        }
        out.extend_from_slice(b"}}\n");

        match action {
            BulkAction::Delete => {}
            BulkAction::Update => {
                out.extend_from_slice(b"{\"doc\":");
                out.extend_from_slice(source_bytes(line, source, is_hit));
                out.extend_from_slice(b"}\n");
            }
            BulkAction::Index | BulkAction::Create => {
                out.extend_from_slice(source_bytes(line, source, is_hit));
                out.push(b'\n');
            }
        }
        Ok(())
    }

    fn bulk_segments(&self) -> Vec<String> {
        match &self.opts.index {
            Some(i) => vec![
                format!("{}{}{}", self.opts.prefix, i, self.opts.suffix),
                "_bulk".to_string(),
            ],
            None => vec!["_bulk".to_string()],
        }
    }

    fn compress(&self, body: Vec<u8>) -> Result<Body> {
        if !self.client.compress_requests() {
            return Ok(Body::Ndjson(Bytes::from(body)));
        }
        let mut enc = flate2::write::GzEncoder::new(
            Vec::with_capacity(body.len() / 4),
            flate2::Compression::fast(),
        );
        enc.write_all(&body)?;
        Ok(Body::NdjsonGzip(Bytes::from(enc.finish()?)))
    }

    /// Send one bulk body. Returns the response and whether the request was sent more than
    /// once (a retry after a failure that may have happened after the cluster applied it).
    async fn send_bulk(&self, body: Vec<u8>, mode: RetryMode) -> Result<(Bytes, bool)> {
        let segments = self.bulk_segments();
        let seg_refs: Vec<&str> = segments.iter().map(String::as_str).collect();
        let seg_refs = &seg_refs;
        let body = self.compress(body)?;
        let body = &body;
        let client = &self.client;
        let sends = std::sync::atomic::AtomicUsize::new(0);
        let sends = &sends;
        let resp = send_with_retry(self.opts.retry, &self.cancel, mode, "bulk", move || {
            sends.fetch_add(1, Ordering::Relaxed);
            client.send(Method::POST, seg_refs, &[], true, body.clone())
        })
        .await?;
        Ok((resp, sends.load(Ordering::Relaxed) > 1))
    }

    /// Documents of `pending` that the cluster would give a generated id (`index`/`create`
    /// without `_id`, e.g. a `--sourceOnly` dump). Re-sending a bulk request that may have been
    /// applied would duplicate them (TST-BRK-NET-003).
    fn auto_id_docs(&self, chunk: &[&[u8]], pending: &[usize]) -> usize {
        if !matches!(self.opts.action, BulkAction::Index | BulkAction::Create) {
            return 0;
        }
        pending
            .iter()
            .filter(|&&i| get(chunk[i], &["_id"]).is_none())
            .count()
    }

    /// Write one batch of document lines; returns once every item was
    /// written, retried or reported.
    async fn process_batch(&mut self, buf: &[u8], first_doc: u64) -> Result<()> {
        // Validate and encode every line first: a line that is not one JSON
        // object (INJ-F1) or whose action fields are unusable (bad `_index`,
        // non-string `_id`, non-integer `version`: INJ-F2/F3) is rejected on
        // its own, so it can never fail the whole bulk request.
        let mut encoded = Vec::with_capacity(buf.len() + buf.len() / 4);
        let mut docs: Vec<(&[u8], std::ops::Range<usize>)> = Vec::new();
        for (no, line) in (first_doc..).zip(nonempty_lines(buf)) {
            let parsed = match check_and_parse(line) {
                Ok(top) => Ok(top),
                Err(CheckErr::Line(reason)) => {
                    self.reject_line(line, &reason, no)?;
                    continue;
                }
                Err(CheckErr::Parse(e)) => Err(e),
            };
            let at = encoded.len();
            match parsed.and_then(|top| self.encode_top(line, &top, &mut encoded)) {
                Ok(()) => docs.push((line, at..encoded.len())),
                Err(e) => {
                    encoded.truncate(at);
                    if e.downcast_ref::<crate::rename::MergeRefused>().is_some()
                        || e.downcast_ref::<FatalDoc>().is_some()
                    {
                        return Err(anyhow!(
                            "{e:#}. Stopped at document {no} of the input before sending its batch; {} document(s) of earlier batches were already written",
                            self.report.written
                        ));
                    }
                    self.reject_line(line, &format!("{e:#}"), no)?;
                }
            }
        }
        let mut start = 0;
        while start < docs.len() {
            // Split oversized batches at --maxBulkBytes. The encoded documents are contiguous
            // in `encoded`, so a chunk is one byte range of it (L-808p: sized first, then
            // moved when it is the whole batch, the common case, instead of copied).
            let mut len = 0usize;
            let mut end = start;
            while end < docs.len() {
                let before = len;
                len += docs[end].1.len();
                end += 1;
                if len >= self.opts.max_bulk_bytes && end - start >= 1 {
                    if len > self.opts.max_bulk_bytes && end - start > 1 {
                        len = before;
                        end -= 1;
                    }
                    break;
                }
            }
            let range = docs[start].1.start..docs[start].1.start + len;
            debug_assert!(end == 0 || range.end == docs[end - 1].1.end);
            let body = if range.start == 0 && range.end == encoded.len() {
                std::mem::take(&mut encoded)
            } else {
                encoded[range].to_vec()
            };
            let chunk: Vec<&[u8]> = docs[start..end].iter().map(|(l, _)| *l).collect();
            match self.write_chunk(&chunk, body).await {
                Ok(()) => {}
                Err(e) if self.opts.ignore_errors && !crate::http::is_cancelled(&e) => {
                    log::error!(
                        "Bulk request failed; skipping {} documents (--ignore-errors): {e:#}",
                        chunk.len()
                    );
                    self.report.failed += chunk.len() as u64;
                }
                Err(e) => return Err(self.rerun_safety(e)),
            }
            start = end;
        }
        if self.opts.refresh && !docs.is_empty() {
            // Skip when another worker's refresh is already running.
            let lock = Arc::clone(&self.refresh_lock);
            if let Ok(_g) = lock.try_lock() {
                self.refresh().await;
            }
        }
        Ok(())
    }

    async fn write_chunk(&mut self, chunk: &[&[u8]], body: Vec<u8>) -> Result<()> {
        let mut pending: Vec<usize> = (0..chunk.len()).collect();
        let mut body = body;
        let mut attempt = 0usize;
        loop {
            // Auto-id batches are not idempotent: they are re-sent only when the cluster
            // provably did not apply them (HTTP 429, or the connection was never made).
            let auto = self.auto_id_docs(chunk, &pending);
            let mode = if auto > 0 {
                RetryMode::RejectionOnly
            } else {
                RetryMode::Idempotent
            };
            let (resp, resent) = match self.send_bulk(body, mode).await {
                Ok(r) => r,
                Err(e)
                    if auto > 0
                        && !crate::http::is_cancelled(&e)
                        && e.downcast_ref::<crate::http::EsHttpError>().is_none() =>
                {
                    return Err(e.context(crate::http::NotRerunSafe(format!(
                        "the bulk request was not re-sent: it holds {auto} document(s) without _id and the connection failed after it was sent, so the cluster may already have indexed them and a retry would duplicate them under new ids. Check the target's document count before re-running; dumps that keep _id (no --sourceOnly) restore retry-safely"
                    ))));
                }
                Err(e) => return Err(e),
            };
            if auto > 0 {
                self.auto_id_written.store(true, Ordering::Relaxed);
            }
            let rejected = self.handle_response(&resp, chunk, &pending, resent)?;
            if rejected.is_empty() {
                return Ok(());
            }
            if attempt >= self.opts.retry.attempts {
                for (i, item) in &rejected {
                    self.report_failure(
                        chunk[*i],
                        &format!("rejected (429/503) after {attempt} retries: {item}"),
                    )?;
                }
                return Ok(());
            }
            let delay = self
                .opts
                .retry
                .delay_for(attempt)
                .max(Duration::from_millis(1));
            log::warn!(
                "{} bulk item(s) rejected with 429/503; retrying them in {:?}",
                rejected.len(),
                delay
            );
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = self.cancel.cancelled() => return Err(anyhow::Error::new(crate::http::DumpCancelled)),
            }
            attempt += 1;
            body = Vec::new();
            for (i, _) in &rejected {
                self.encode_doc(chunk[*i], &mut body)?;
            }
            pending = rejected.into_iter().map(|(i, _)| i).collect();
        }
    }

    /// Refuse a line that is not exactly one JSON object. It is never
    /// encoded (no partial action can reach the cluster). With
    /// --ignore-errors/--ignore-es-write-errors the run continues, but the
    /// rejection is counted and the run still exits non-zero.
    fn reject_line(&mut self, line: &[u8], reason: &str, no: u64) -> Result<()> {
        self.report.failed += 1;
        self.report.rejected += 1;
        let msg = format!(
            "rejected document {no} of the input (line {no} of an NDJSON dump) ({reason}); nothing was written for it: {}",
            preview(line)
        );
        if self.opts.ignore_errors || self.opts.ignore_es_write_errors {
            log::error!("{msg}");
            Ok(())
        } else {
            Err(anyhow!(msg))
        }
    }

    /// L-804b: a transient failure after documents without `_id` were sent is not safe to
    /// rerun (they would be indexed again under new ids): say so, and keep exit status 1.
    fn rerun_safety(&self, e: anyhow::Error) -> anyhow::Error {
        if self.auto_id_written.load(Ordering::Relaxed) && crate::is_transient(&e) {
            e.context(crate::http::NotRerunSafe(
                "documents without _id were already written by this run, so a rerun would index them again under new ids; check the target's document count before re-running".to_string(),
            ))
        } else {
            e
        }
    }

    fn report_failure(&mut self, doc: &[u8], item: &str) -> Result<()> {
        self.report.failed += 1;
        // L-804b (P5): count refusals by an index/cluster block, and those of them from the
        // disk flood-stage watermark, so the exit-3 summary can name the dominant cause
        if item.contains("cluster_block_exception") {
            self.report.blocked += 1;
            if is_disk_block(item) {
                self.report.disk_blocked += 1;
            }
        }
        // name the document that was sent, not what the item echoes (TST-BRK-SRV-003)
        let id = get(doc, &["_id"])
            .map(|v| v.as_raw_str().to_string())
            .unwrap_or_else(|| "<auto id>".into());
        if self.opts.ignore_es_write_errors {
            eprintln!(
                "not written: document {id}: {}",
                crate::secrets::scrub(item)
            );
            Ok(())
        } else {
            Err(anyhow!("bulk write failed for document {id}: {item}"))
        }
    }

    /// Fuzz hook (TST-API-084): bulk response accounting for `chunk`.
    #[cfg(any(feature = "fuzz", test))]
    pub fn fuzz_handle_response(&mut self, resp: &[u8], chunk: &[&[u8]]) -> Result<()> {
        let pending: Vec<usize> = (0..chunk.len()).collect();
        for resent in [false, true] {
            let _ = self.handle_response(resp, chunk, &pending, resent);
        }
        Ok(())
    }

    /// Account the response for `pending` (indices into `chunk`); returns the
    /// item indices rejected with 429/503 for retry, with the item's response.
    fn handle_response(
        &mut self,
        resp: &[u8],
        chunk: &[&[u8]],
        pending: &[usize],
        resent: bool,
    ) -> Result<Vec<(usize, String)>> {
        // Never count a document as written without its own item (TST-BRK-SRV-004): an
        // answer that is not JSON, has no `items`, or has a different number of items fails
        // the run, whatever `errors` says.
        let what = |m: &str| {
            let head: String = String::from_utf8_lossy(&resp[..resp.len().min(120)])
                .chars()
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            anyhow!(
                "{m} for {} document(s); none of them is counted as written (response starts: {head:?})",
                pending.len()
            )
        };
        if !resp.trim_ascii_start().starts_with(b"{") {
            return Err(what("the bulk response is not a JSON object"));
        }
        let errors = get(resp, &["errors"]).and_then(|v| v.as_bool());
        let items = get(resp, &["items"]).ok_or_else(|| what("the bulk response has no items"))?;
        let items: Vec<LazyValue> = sonic_rs::to_array_iter(items.as_raw_str().as_bytes())
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| what("the bulk response items are not an array"))?;
        if items.len() != pending.len() {
            return Err(what(&format!(
                "the bulk response has {} item(s)",
                items.len()
            )));
        }
        if errors == Some(false) {
            self.report.written += pending.len() as u64;
            if self.opts.action == BulkAction::Index {
                self.report.overwritten += items
                    .iter()
                    .filter(|i| item_status_any_key(i.as_raw_str().as_bytes()) == Some(200))
                    .count() as u64;
            }
            return Ok(vec![]);
        }
        let mut rejected = Vec::new();
        let mut created_earlier = 0u64;
        for (item, &doc_idx) in items.iter().zip(pending) {
            let raw = item.as_raw_str().as_bytes();
            let action = self.opts.action.as_str();
            // the item is keyed by the action, but data streams (and some proxies) answer
            // under another key (`create` for `index`): use the single key's status then
            let status = get(raw, &[action, "status"])
                .and_then(|v| v.as_u64())
                .or_else(|| item_status_any_key(raw))
                .unwrap_or(0);
            if (200..400).contains(&status) {
                self.report.written += 1;
                if status == 200 && self.opts.action == BulkAction::Index {
                    self.report.overwritten += 1;
                }
            } else if status == 409 && resent && self.opts.action == BulkAction::Create {
                // the request was sent again after a failure; the first attempt most likely
                // created the document (TST-BRK-NET-003)
                self.report.written += 1;
                created_earlier += 1;
            } else if (status == 429 || status == 503) && self.opts.retry.attempts > 0 {
                rejected.push((doc_idx, item.as_raw_str().to_string()));
            } else {
                if status == 400
                    && !self.data_stream_hint_shown
                    && self.opts.action == BulkAction::Index
                    && item
                        .as_raw_str()
                        .contains("op_type of create are allowed in data streams")
                {
                    // L-656: name the fix once, not per document
                    self.data_stream_hint_shown = true;
                    log::warn!(
                        "the target is a data stream, which accepts only `create`: every document written with --bulkAction index fails; re-run with --bulkAction create"
                    );
                }
                if status == 409 && !self.conflict_hint_shown {
                    // TST-BRK-SRV-011: name the concurrent change once, not per document
                    self.conflict_hint_shown = true;
                    log::warn!(
                        "version conflict (409): a document was created or changed by another writer (a concurrent restore, or an earlier run) between reads; it is counted as failed{}",
                        if self.opts.action == BulkAction::Create {
                            " (`--bulkAction index` overwrites instead of `create`)"
                        } else {
                            ""
                        }
                    );
                }
                self.report_failure(chunk[doc_idx], item.as_raw_str())?;
            }
        }
        if created_earlier > 0 {
            log::warn!(
                "{created_earlier} `create` item(s) answered 409 after the bulk request was re-sent; counted as written by the earlier attempt (a document that existed before the run would be counted the same way)"
            );
        }
        Ok(rejected)
    }

    async fn refresh(&self) {
        let segments = match &self.opts.index {
            Some(i) => vec![
                format!("{}{}{}", self.opts.prefix, i, self.opts.suffix),
                "_refresh".into(),
            ],
            None => vec!["_refresh".to_string()],
        };
        let refs: Vec<&str> = segments.iter().map(String::as_str).collect();
        match self
            .client
            .send(Method::POST, &refs, &[], false, Body::Empty)
            .await
        {
            Ok(r) if r.status().is_success() => {}
            Ok(r) => log::warn!("refresh failed: HTTP {}", r.status()),
            Err(e) => log::warn!("refresh failed: {e}"),
        }
    }
}

/// `status` of a bulk item `{"<any action>": {"status": N, …}}` with exactly one key.
fn item_status_any_key(raw: &[u8]) -> Option<u64> {
    let mut it = sonic_rs::to_object_iter(raw);
    let (_, v) = it.next()?.ok()?;
    if it.next().is_some() {
        return None;
    }
    get(v.as_raw_str().as_bytes(), &["status"]).and_then(|s| s.as_u64())
}

fn source_bytes<'a>(line: &'a [u8], source: Option<&'a LazyValue<'_>>, is_hit: bool) -> &'a [u8] {
    match source {
        Some(s) => s.as_raw_str().as_bytes(),
        // a hit without _source (e.g. stored_fields only): empty document
        None if is_hit => b"{}",
        // a plain document (e.g. a --sourceOnly dump): the whole line
        None => line,
    }
}

#[cfg(test)]
mod tests {
    /// L-808n: the memchr line splitter matches the byte-by-byte split it replaced.
    #[test]
    fn nonempty_lines_matches_split_filter() {
        for buf in [
            &b""[..],
            b"\n",
            b"\n\n\n",
            b"a",
            b"a\n",
            b"\na",
            b"a\nb",
            b"a\n\nb\n",
            b"{\"x\":1}\n{\"y\":2}",
            b"\n\n{}\n\n{}\n\n",
        ] {
            let want: Vec<&[u8]> = buf
                .split(|b| *b == b'\n')
                .filter(|l| !l.is_empty())
                .collect();
            let got: Vec<&[u8]> = nonempty_lines(buf).collect();
            assert_eq!(got, want, "{:?}", String::from_utf8_lossy(buf));
        }
    }

    /// L-808n: the bracket-count shortcut never changes the depth verdict.
    /// L-808o: `check_and_parse` gives the verdict, message and members of `check_doc_line`
    /// followed by `Top::parse` on valid lines, malformed ones and byte-level mutations.
    #[test]
    fn check_and_parse_matches_the_two_pass_path() {
        fn old(line: &[u8]) -> String {
            match check_doc_line(line) {
                Err(r) => format!("line: {r}"),
                Ok(()) => match Top::parse(line) {
                    Err(e) => format!("parse: {e:#}"),
                    Ok(t) => format!("ok: {:?}", members(&t)),
                },
            }
        }
        fn new(line: &[u8]) -> String {
            match check_and_parse(line) {
                Err(CheckErr::Line(r)) => format!("line: {r}"),
                Err(CheckErr::Parse(e)) => format!("parse: {e:#}"),
                Ok(t) => format!("ok: {:?}", members(&t)),
            }
        }
        fn members(t: &Top) -> Vec<(String, String)> {
            t.members
                .iter()
                .map(|(k, v)| (k.to_string(), v.as_raw_str().to_string()))
                .collect()
        }
        let seeds: &[&str] = &[
            r#"{"_index":"a","_id":"1","_source":{"x":[1,2.5e3,{"y":"z\u00e9\n"}],"t":true,"n":null}}"#,
            r#"{}"#,
            r#"{ }"#,
            "\t{ \"a\" : 1 }\t ",
            r#"{"a":1}  "#,
            r#"{"a":1}x"#,
            r#"{"a":1}}"#,
            r#"{"a":1,}"#,
            r#"{"a":1}{"b":2}"#,
            r#"{"a":"\uZZZZ"}"#,
            r#"{"a":"\q"}"#,
            r#"{"a":1,"a":2}"#,
            r#"{"a":nul}"#,
            r#"{"a":01}"#,
            r#"{"a":-}"#,
            r#"{"a":1e}"#,
            r#"{"a":"x"#,
            r#"{"a" 1}"#,
            r#"{"a":[1,2}"#,
            r#"{"a":{"b":{"c":[[[]]]}}}"#,
            r#"{"_source":"s","_id":7,"n":null}"#,
            r#"{"k\"q":"v\\"}"#,
            "{\"a\":\"\u{1}\"}",
            "{\"a\":\"\u{e9}\"}",
            r#"[1]"#,
            r#""s""#,
        ];
        let mut lines: Vec<Vec<u8>> = seeds.iter().map(|s| s.as_bytes().to_vec()).collect();
        lines.push(b"{\"a\":\"\xff\"}".to_vec());
        lines.push(b"{\"a\":1}\x00".to_vec());
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = |n: usize| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % n as u64) as usize
        };
        let alphabet = b" \t{}[]\",:\\0123456789-+.eEtrufalsn\xff\x00ab";
        for seed in seeds {
            for _ in 0..400 {
                let mut l = seed.as_bytes().to_vec();
                for _ in 0..1 + rnd(3) {
                    let i = rnd(l.len() + 1);
                    match rnd(3) {
                        0 if i < l.len() => l[i] = alphabet[rnd(alphabet.len())],
                        1 if i < l.len() => {
                            l.remove(i);
                        }
                        _ => l.insert(i, alphabet[rnd(alphabet.len())]),
                    }
                }
                lines.push(l);
            }
        }
        for l in &lines {
            assert_eq!(new(l), old(l), "{:?}", String::from_utf8_lossy(l));
        }
    }

    #[test]
    fn too_deep_shortcut_keeps_the_verdict() {
        fn slow(line: &[u8], max: usize) -> Option<usize> {
            let (mut depth, mut in_str, mut esc) = (0usize, false, false);
            for (i, &b) in line.iter().enumerate() {
                if in_str {
                    if esc {
                        esc = false;
                    } else if b == b'\\' {
                        esc = true;
                    } else if b == b'"' {
                        in_str = false;
                    }
                    continue;
                }
                match b {
                    b'"' => in_str = true,
                    b'[' | b'{' => {
                        depth += 1;
                        if depth > max {
                            return Some(i);
                        }
                    }
                    b']' | b'}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            None
        }
        let deep = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        let cases = [
            deep(3),
            deep(4),
            deep(5),
            format!("{{\"s\":\"{}\"}}", "[".repeat(10)),
            format!("{{\"a\":{}}}", deep(3)),
            "[[]][[]][[]]".to_string(),
            "{}".to_string(),
            String::new(),
        ];
        for line in &cases {
            for max in [0, 1, 2, 3, 4, 5, 10] {
                assert_eq!(
                    too_deep(line.as_bytes(), max),
                    slow(line.as_bytes(), max),
                    "{line} max {max}"
                );
            }
        }
        assert_eq!(too_deep(deep(1001).as_bytes(), MAX_DOC_DEPTH), Some(1000));
        assert_eq!(too_deep(deep(1000).as_bytes(), MAX_DOC_DEPTH), None);
    }

    use super::*;
    use crate::http::{ClientOptions, Flavor, gate::RequestGate};

    fn sink(info: ClusterInfo, f: impl FnOnce(&mut BulkOptions)) -> BulkSink {
        let client = EsClient::new(
            &ClientOptions::new(url::Url::parse("http://localhost:9200/").unwrap()),
            RequestGate::default(),
        )
        .unwrap();
        let mut o = BulkOptions {
            info,
            index: None,
            doc_type: None,
            prefix: String::new(),
            suffix: String::new(),
            rename: None,
            action: BulkAction::Index,
            handle_version: false,
            version_type: None,
            meta_fields: vec![
                "routing".into(),
                "parent".into(),
                "timestamp".into(),
                "ttl".into(),
            ],
            refresh: true,
            ignore_es_write_errors: true,
            ignore_errors: false,
            max_bulk_bytes: 100 << 20,
            retry: RetryPolicy::none(),
        };
        f(&mut o);
        BulkSink::new(client, o, CancellationToken::new())
    }

    fn enc(s: &mut BulkSink, line: &str) -> String {
        let mut out = vec![];
        s.encode_doc(line.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn os() -> ClusterInfo {
        ClusterInfo::new(Flavor::OpenSearch, "2.19.0")
    }

    #[test]
    fn meta_lookup_order_matches_elasticdump() {
        // TST-API-015: `X`, then `_X`, then `fields.X`, then `fields._X`; one-element arrays unwrap
        let t = Top::parse(
            br#"{"routing":"a","_routing":"b","fields":{"routing":["c"],"_routing":["d"]}}"#,
        )
        .unwrap();
        assert_eq!(super::meta(&t, "routing").as_deref(), Some("\"a\""));
        let t =
            Top::parse(br#"{"_routing":"b","fields":{"routing":["c"],"_routing":["d"]}}"#).unwrap();
        assert_eq!(super::meta(&t, "routing").as_deref(), Some("\"b\""));
        let t = Top::parse(br#"{"fields":{"routing":["c"],"_routing":["d"]}}"#).unwrap();
        assert_eq!(super::meta(&t, "routing").as_deref(), Some("\"c\""));
        let t = Top::parse(br#"{"fields":{"_routing":["d"]}}"#).unwrap();
        assert_eq!(super::meta(&t, "routing").as_deref(), Some("\"d\""));
        let t = Top::parse(br#"{"fields":{"_routing":["d","e"]}}"#).unwrap();
        assert_eq!(
            super::meta(&t, "routing"),
            None,
            "multi-valued is not a meta value"
        );
    }

    /// L-629a: numeric ids and routing reach the action line as the exact literal text;
    /// JSON whitespace around the value is not part of it.
    #[test]
    fn numeric_ids_reach_the_action_line_byte_for_byte() {
        let mut s = sink(os(), |_| {});
        for (id, want) in [
            ("-0", r#""-0""#),
            (" 1", r#""1""#),
            ("1e3", r#""1e3""#),
            ("0.0", r#""0.0""#),
            ("\"00\"", r#""00""#),
            ("\" 1\"", r#"" 1""#),
            ("\"+1\"", r#""+1""#),
            ("\"NaN\"", r#""NaN""#),
        ] {
            let line = format!(r#"{{"_index":"i","_id":{id},"_routing": -0 ,"_source":{{}}}}"#);
            let got = enc(&mut s, &line);
            assert!(
                got.starts_with(&format!(
                    r#"{{"index":{{"_index":"i","_id":{want},"routing":"-0"}}}}"#
                )),
                "{id}: {got}"
            );
        }
        for bad in ["00", "+1", "NaN"] {
            let line = format!(r#"{{"_index":"i","_id":{bad},"_source":{{}}}}"#);
            let mut out = vec![];
            assert!(s.encode_doc(line.as_bytes(), &mut out).is_err(), "{bad}");
            assert!(out.is_empty(), "{bad}: nothing encoded");
        }
    }

    #[test]
    fn index_action_keeps_raw_source_and_routing() {
        let mut s = sink(os(), |_| {});
        assert_eq!(
            enc(
                &mut s,
                r#"{"_index":"src","_type":"_doc","_id":"a\"b","_routing":"r1","_source":{"n":123456789012345678901234567890}}"#
            ),
            "{\"index\":{\"_index\":\"src\",\"_id\":\"a\\\"b\",\"routing\":\"r1\"}}\n{\"n\":123456789012345678901234567890}\n"
        );
    }

    #[test]
    fn output_index_prefix_suffix_and_types_for_old_es() {
        let mut s = sink(ClusterInfo::new(Flavor::Elastic, "6.8.0"), |o| {
            o.index = Some("dst".into());
            o.prefix = "p_".into();
            o.suffix = "_s".into();
        });
        assert_eq!(
            enc(
                &mut s,
                r#"{"_index":"src","_type":"doc","_id":"1","_source":{}}"#
            ),
            "{\"index\":{\"_index\":\"p_dst_s\",\"_id\":\"1\",\"_type\":\"doc\"}}\n{}\n"
        );
    }

    #[test]
    fn update_delete_create_and_versions() {
        let mut s = sink(os(), |o| {
            o.action = BulkAction::Update;
        });
        assert_eq!(
            enc(&mut s, r#"{"_index":"i","_id":"1","_source":{"a":1}}"#),
            "{\"update\":{\"_index\":\"i\",\"_id\":\"1\"}}\n{\"doc\":{\"a\":1}}\n"
        );
        let mut s = sink(os(), |o| o.action = BulkAction::Delete);
        assert_eq!(
            enc(&mut s, r#"{"_index":"i","_id":"1","_source":{"a":1}}"#),
            "{\"delete\":{\"_index\":\"i\",\"_id\":\"1\"}}\n"
        );
        let mut s = sink(os(), |o| {
            o.action = BulkAction::Create;
            o.handle_version = true;
            o.version_type = Some("external".into());
        });
        assert_eq!(
            enc(
                &mut s,
                r#"{"_index":"i","_id":"1","_version":7,"_source":{}}"#
            ),
            "{\"create\":{\"_index\":\"i\",\"_id\":\"1\",\"version\":7,\"version_type\":\"external\"}}\n{}\n"
        );
    }

    #[test]
    fn plain_documents_need_an_output_index() {
        let mut s = sink(os(), |_| {});
        let mut out = vec![];
        assert!(s.encode_doc(br#"{"a":1}"#, &mut out).is_err());
        let mut s = sink(os(), |o| o.index = Some("t".into()));
        assert_eq!(
            enc(&mut s, r#"{"a":1}"#),
            "{\"index\":{\"_index\":\"t\"}}\n{\"a\":1}\n"
        );
    }

    #[test]
    fn extra_fields_and_ttl_drop() {
        let mut s = sink(os(), |o| o.meta_fields.push("pipeline".into()));
        assert_eq!(
            enc(
                &mut s,
                r#"{"_index":"i","_id":"1","_ttl":5,"pipeline":"p","_source":{}}"#
            ),
            "{\"index\":{\"_index\":\"i\",\"_id\":\"1\",\"pipeline\":\"p\"}}\n{}\n"
        );
    }

    #[test]
    fn response_accounting() {
        let mut s = sink(os(), |o| o.ignore_es_write_errors = true);
        let docs: Vec<&[u8]> = vec![b"{}", b"{}", b"{}"];
        let r = s
            .handle_response(
                br#"{"errors":false,"items":[{},{},{}]}"#,
                &docs,
                &[0, 1, 2],
                false,
            )
            .unwrap();
        assert!(r.is_empty());
        assert_eq!(s.report.written, 3);
        let r = s
            .handle_response(
                br#"{"errors":true,"items":[{"index":{"status":201}},{"index":{"status":400,"error":{"type":"mapper_parsing_exception"}}},{"index":{"status":429}}]}"#,
                &docs,
                &[0, 1, 2],
                false,
            )
            .unwrap();
        assert!(r.is_empty(), "no retries configured");
        assert_eq!(
            s.report,
            WriteReport {
                written: 4,
                failed: 2,
                ..Default::default()
            }
        );

        let mut s = sink(os(), |o| {
            o.ignore_es_write_errors = false;
        });
        assert!(
            s.handle_response(
                br#"{"errors":true,"items":[{"index":{"status":400}}]}"#,
                &docs[..1],
                &[0],
                false
            )
            .is_err()
        );

        let mut s = sink(os(), |o| o.retry.attempts = 2);
        let r = s
            .handle_response(
                br#"{"errors":true,"items":[{"index":{"status":201}},{"index":{"status":429}},{"index":{"status":503}}]}"#,
                &docs,
                &[0, 1, 2],
                false,
            )
            .unwrap();
        assert_eq!(r.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn create_409_after_a_resend_counts_as_written() {
        let docs: Vec<&[u8]> = vec![
            br#"{"_id":"1","_source":{}}"#,
            br#"{"_id":"2","_source":{}}"#,
        ];
        let conflict = br#"{"errors":true,"items":[{"create":{"status":201}},{"create":{"status":409,"error":{"type":"version_conflict_engine_exception"}}}]}"#;
        let mut s = sink(os(), |o| {
            o.action = BulkAction::Create;
            o.ignore_es_write_errors = true;
        });
        s.handle_response(conflict, &docs, &[0, 1], true).unwrap();
        assert_eq!((s.report.written, s.report.failed), (2, 0));
        // without a re-send a 409 is a real conflict
        s.handle_response(conflict, &docs, &[0, 1], false).unwrap();
        assert_eq!((s.report.written, s.report.failed), (3, 1));
    }

    #[test]
    fn auto_id_docs_are_counted_for_index_and_create_only() {
        let docs: Vec<&[u8]> = vec![
            br#"{"a":1}"#,
            br#"{"_id":"x","_source":{}}"#,
            br#"{"_source":{"b":2}}"#,
        ];
        let s = sink(os(), |_| {});
        assert_eq!(s.auto_id_docs(&docs, &[0, 1, 2]), 2);
        assert_eq!(s.auto_id_docs(&docs, &[1]), 0);
        let s = sink(os(), |o| o.action = BulkAction::Delete);
        assert_eq!(s.auto_id_docs(&docs, &[0, 2]), 0);
    }
}

#[cfg(test)]
mod inj_f1_tests {
    use super::*;

    /// TST-INJ-003: a line that is not exactly one complete JSON object is
    /// refused before encoding; no partial `Top` is ever produced.
    #[test]
    fn partial_or_trailing_lines_are_rejected() {
        for bad in [
            &br#"{"_index":"src","_id":"x","_score":1.0,"_source":{"a":"#[..],
            br#"{"_index":"victim","_id":"admin","_source":{"role":"superuser"}}}}"#,
            br#"}}"#,
            br#"}"#,
            br#"[1,2]"#,
            br#""str""#,
            br#"{"a":1} {"b":2}"#,
            br#"{"a":1}x"#,
            b"{\"a\":\n1}",
        ] {
            assert!(
                check_doc_line(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        for good in [
            &br#"{"a":1}"#[..],
            br#"  {"_index":"i","_id":"1","_source":{"s":"}\"{"}}  "#,
            br#"{}"#,
        ] {
            assert!(
                check_doc_line(good).is_ok(),
                "{}",
                String::from_utf8_lossy(good)
            );
        }
        assert!(Top::parse(br#"{"_index":"victim","_id":"admin","_source":"#).is_err());
    }

    /// TST-INJ-009: raw control bytes inside strings are not valid JSON.
    #[test]
    fn raw_control_bytes_are_rejected() {
        assert!(check_doc_line(b"{\"a\":\"x\x01y\"}").is_err());
        assert!(check_doc_line(b"{\"a\":\"x\x00y\"}").is_err());
        assert!(check_doc_line(b"{\"a\":\"x\\u0001y\"}").is_ok());
        // L-505 fuzz finding: invalid escapes inside string values
        for bad in [
            &br#"{"a":"\u%%%%"}"#[..],
            br#"{"a":"\u12"}"#,
            br#"{"a":["x\q"]}"#,
            br#"{"a":{"b":"\u00zz"}}"#,
        ] {
            assert!(
                check_doc_line(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        for good in [
            &br#"{"a":"\\u%%"}"#[..],
            br#"{"a":"\u00e9\n\t\"\/\\"}"#,
            br#"{"a\u0062":"\uD83D\uDE00"}"#,
        ] {
            assert!(
                check_doc_line(good).is_ok(),
                "{}",
                String::from_utf8_lossy(good)
            );
        }
    }

    /// TST-INJ-008: duplicate top-level keys are found.
    #[test]
    fn duplicate_keys_are_detected() {
        let t = Top::parse(br#"{"_index":"a","_id":"1","_index":"victim"}"#).unwrap();
        assert_eq!(t.duplicate_key(), Some("_index"));
        let t = Top::parse(br#"{"_index":"a","_id":"1"}"#).unwrap();
        assert_eq!(t.duplicate_key(), None);
    }
}

#[cfg(test)]
mod data_stream_tests {
    use super::data_stream_of;

    #[test]
    fn backing_indices_map_back_to_their_stream() {
        for (idx, want) in [
            (
                ".ds-logs-app-default-2026.10.30-000001",
                Some("logs-app-default"),
            ),
            (".ds-logs-000002", Some("logs")),
            (
                ".ds-my-stream-2026.1.30-000001",
                Some("my-stream-2026.1.30"),
            ),
            (".ds-x-2026.10.30-00001", None),
            ("logs-app", None),
            (".ds--000001", None),
            (".ds-", None),
        ] {
            assert_eq!(data_stream_of(idx), want, "{idx}");
        }
    }
}

/// L-510 (TST-INJ-004): encoder differential proptest. Arbitrary hits (keys, escapes, value
/// types, numeric or string ids/routing, versions) for every bulk action, with and without
/// an output index and on typed/typeless clusters: `encode_doc` gives 2 NDJSON lines per doc
/// (1 for delete), only allowed action keys, the expected `_index`, and equals a reference
/// encoder built with serde_json. 100k cases by default (`PROPTEST_CASES` overrides).
#[cfg(test)]
mod prop_bulk {
    use super::*;
    use crate::http::{ClientOptions, Flavor, gate::RequestGate};
    use proptest::prelude::*;
    use serde_json::{Map, Value, json};

    fn cases() -> u32 {
        std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100_000)
    }

    fn text() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                Just('"'),
                Just('\\'),
                Just('/'),
                Just('\n'),
                Just('\u{1}'),
                Just('\u{2028}'),
                Just('é'),
                Just('😀'),
                Just('{'),
                Just('}'),
                Just(','),
                Just(':'),
                proptest::char::range('a', 'e'),
            ],
            0..8,
        )
        .prop_map(|v| v.into_iter().collect())
    }

    /// A JSON literal for `s`, using `\u` escapes for some characters.
    fn lit(s: &str, esc: bool) -> String {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c if esc && (c as u32) < 0x10000 && c != '/' => {
                    o.push_str(&format!("\\u{:04X}", c as u32))
                }
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }

    fn number() -> impl Strategy<Value = String> {
        prop_oneof![
            any::<i64>().prop_map(|n| n.to_string()),
            any::<u64>().prop_map(|n| n.to_string()),
            Just("1.0".to_string()),
            Just("-0".to_string()),
            Just("1e400".to_string()),
            Just("123456789012345678901234567890".to_string()),
        ]
    }

    fn value() -> impl Strategy<Value = String> {
        let leaf = prop_oneof![
            (text(), any::<bool>()).prop_map(|(s, e)| lit(&s, e)),
            number(),
            Just("true".to_string()),
            Just("null".to_string()),
        ];
        leaf.prop_recursive(3, 24, 4, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4)
                    .prop_map(|v| format!("[{}]", v.join(","))),
                proptest::collection::btree_map(text(), inner, 0..4).prop_map(|m| {
                    let parts: Vec<String> = m
                        .iter()
                        .map(|(k, v)| format!("{}: {v}", lit(k, false)))
                        .collect();
                    format!("{{ {} }}", parts.join(" ,"))
                }),
            ]
        })
    }

    fn source() -> impl Strategy<Value = String> {
        proptest::collection::btree_map(text(), value(), 0..5).prop_map(|m| {
            let parts: Vec<String> = m
                .iter()
                .map(|(k, v)| format!("{}:{v}", lit(k, false)))
                .collect();
            format!("{{{}}}", parts.join(","))
        })
    }

    #[derive(Debug, Clone)]
    struct Case {
        line: String,
        action: BulkAction,
        out_index: Option<String>,
        typed: bool,
        handle_version: bool,
        version_type: Option<String>,
    }

    fn case() -> impl Strategy<Value = Case> {
        let id = prop_oneof![
            (text(), any::<bool>()).prop_map(|(s, e)| lit(&format!("i{s}"), e)),
            any::<u32>().prop_map(|n| n.to_string()),
        ];
        let routing = proptest::option::of(prop_oneof![
            (text(), any::<bool>()).prop_map(|(s, e)| lit(&s, e)),
            any::<u16>().prop_map(|n| n.to_string()),
        ]);
        (
            prop_oneof![Just("logs"), Just("a-b.c"), Just("é-idx"), Just("x_1")],
            proptest::option::of(id),
            routing,
            proptest::option::of(any::<u64>()),
            proptest::option::of(source()),
            proptest::option::of(value()),
            prop_oneof![
                Just(BulkAction::Index),
                Just(BulkAction::Create),
                Just(BulkAction::Update),
                Just(BulkAction::Delete)
            ],
            proptest::option::of(Just("dst".to_string())),
            any::<bool>(),
            any::<bool>(),
            proptest::option::of(Just("external".to_string())),
        )
            .prop_filter("a hit needs _id or _source", |t| {
                t.1.is_some() || t.4.is_some()
            })
            .prop_map(
                |(idx, id, routing, version, src, score, action, out_index, typed, hv, vt)| {
                    let mut members = vec![format!("\"_index\":{}", lit(idx, false))];
                    if let Some(id) = id {
                        members.push(format!("\"_id\":{id}"));
                    }
                    if let Some(r) = routing {
                        members.push(format!("\"_routing\":{r}"));
                    }
                    if let Some(v) = version {
                        members.push(format!("\"_version\":{v}"));
                    }
                    if let Some(s) = score {
                        members.push(format!("\"_score\":{s}"));
                    }
                    members.push(format!(
                        "\"_source\":{}",
                        src.unwrap_or_else(|| "{}".into())
                    ));
                    Case {
                        line: format!("{{{}}}", members.join(",")),
                        action,
                        out_index,
                        typed,
                        handle_version: hv,
                        version_type: vt,
                    }
                },
            )
    }

    fn sink(c: &Case) -> BulkSink {
        let client = EsClient::new(
            &ClientOptions::new(url::Url::parse("http://localhost:9200/").unwrap()),
            RequestGate::default(),
        )
        .unwrap();
        let info = if c.typed {
            ClusterInfo::new(Flavor::Elastic, "6.8.23")
        } else {
            ClusterInfo::new(Flavor::OpenSearch, "2.19.0")
        };
        BulkSink::new(
            client,
            BulkOptions {
                info,
                index: c.out_index.clone(),
                doc_type: None,
                prefix: String::new(),
                suffix: String::new(),
                rename: None,
                action: c.action,
                handle_version: c.handle_version,
                version_type: c.version_type.clone(),
                meta_fields: vec!["routing".into(), "parent".into()],
                refresh: true,
                ignore_es_write_errors: true,
                ignore_errors: false,
                max_bulk_bytes: 100 << 20,
                retry: RetryPolicy::none(),
            },
            CancellationToken::new(),
        )
    }

    thread_local! {
        static SINKS: std::cell::RefCell<std::collections::HashMap<String, BulkSink>> =
            Default::default();
    }

    /// Building an HTTP client per case dominated the run time: one sink per configuration.
    fn with_sink<T>(c: &Case, f: impl FnOnce(&mut BulkSink) -> T) -> T {
        let key = format!(
            "{:?}{:?}{}{}{:?}",
            c.action, c.out_index, c.typed, c.handle_version, c.version_type
        );
        SINKS.with(|m| {
            let mut m = m.borrow_mut();
            f(m.entry(key).or_insert_with(|| sink(c)))
        })
    }

    /// The reference: the same rules, written over serde_json values.
    fn reference(c: &Case) -> Vec<Value> {
        let doc: Map<String, Value> = serde_json::from_str(&c.line).unwrap();
        let as_id = |v: &Value| match v {
            Value::String(s) => Value::String(s.clone()),
            other => Value::String(other.to_string()),
        };
        let mut meta = Map::new();
        meta.insert(
            "_index".into(),
            json!(
                c.out_index
                    .clone()
                    .unwrap_or_else(|| doc["_index"].as_str().unwrap().to_string())
            ),
        );
        if let Some(id) = doc.get("_id") {
            meta.insert("_id".into(), as_id(id));
        }
        if c.typed {
            meta.insert("_type".into(), json!("_doc"));
        }
        if let Some(r) = doc.get("_routing") {
            meta.insert("routing".into(), as_id(r));
        }
        if c.handle_version
            && let Some(v) = doc.get("_version")
        {
            meta.insert("version".into(), v.clone());
            if let Some(vt) = &c.version_type {
                meta.insert("version_type".into(), json!(vt));
            }
        }
        let action = json!({ c.action.as_str(): meta });
        let src = doc["_source"].clone();
        match c.action {
            BulkAction::Delete => vec![action],
            BulkAction::Update => vec![action, json!({ "doc": src })],
            BulkAction::Index | BulkAction::Create => vec![action, src],
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

        #[test]
        fn encode_doc_matches_a_serde_json_reference(c in case()) {
            prop_assert!(check_doc_line(c.line.as_bytes()).is_ok(), "generator: {}", c.line);
            let mut out = Vec::new();
            with_sink(&c, |s| s.encode_doc(c.line.as_bytes(), &mut out)).unwrap();
            let text = std::str::from_utf8(&out).unwrap();
            prop_assert!(text.ends_with('\n'));
            let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
            let want = reference(&c);
            prop_assert_eq!(lines.len(), want.len(), "{}", text);
            let got: Vec<Value> = lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
            let action = got[0].as_object().unwrap();
            prop_assert_eq!(action.len(), 1);
            let allowed = ["_index", "_id", "_type", "routing", "parent", "version", "version_type"];
            for k in action.values().next().unwrap().as_object().unwrap().keys() {
                prop_assert!(allowed.contains(&k.as_str()), "action key {}", k);
            }
            prop_assert_eq!(&got, &want, "{} -> {}", c.line, text);
        }
    }
}

/// Restores that have not run this long skip the target-index check (TST-BRK-SRV-008), so
/// short runs send exactly Node's requests.
/// TST-BRK-SRV-008: what happened to the single target index. `noted` is the uuid noted
/// mid-run (if any), `gone_mid_run` whether it was missing then, `end` the lookup at the
/// end (`Err` = 404). `None`: unchanged; `Some(false)`: deleted; `Some(true)`: deleted and
/// re-created (auto-create, dynamic mappings).
fn target_index_outcome(
    noted: Option<&str>,
    gone_mid_run: bool,
    end: std::result::Result<Option<String>, ()>,
) -> Option<bool> {
    match (noted, end) {
        (_, Err(())) if noted.is_some() || gone_mid_run => Some(false),
        (Some(start), Ok(Some(end))) if end != start => Some(true),
        (None, Ok(_)) if gone_mid_run => Some(true),
        _ => None,
    }
}

const UUID_CAPTURE_AFTER: Duration = Duration::from_secs(5);

#[cfg(test)]
mod id_literal_tests {
    use super::{is_json_number, push_id_like_raw};

    fn id(raw: &str) -> anyhow::Result<String> {
        let mut out = Vec::new();
        push_id_like_raw(&mut out, "_id", raw)?;
        Ok(String::from_utf8(out).unwrap())
    }

    /// L-629a: number literals become a string of exactly their text; nothing is parsed
    /// as a number, normalized, rounded or refused for range.
    #[test]
    fn numeric_ids_are_kept_byte_for_byte() {
        for raw in [
            "-0",
            "0",
            "0.0",
            "-0.0",
            "1e3",
            "1E+3",
            "-2.5E2",
            "1.50",
            "1e21",
            "1e400",
            "-1e-400",
            "12345678901234567890123",
            "18446744073709551617",
            "0.1000000000000000055511151231257827",
        ] {
            assert_eq!(id(raw).unwrap(), format!("\"{raw}\""), "{raw}");
        }
    }

    /// String ids that look like numbers (or not quite) are sent exactly as given,
    /// escapes included.
    #[test]
    fn number_like_string_ids_are_kept_byte_for_byte() {
        for raw in [
            r#""-0""#,
            r#""0.0""#,
            r#""1e3""#,
            r#""00""#,
            r#"" 1""#,
            r#""+1""#,
            r#""NaN""#,
            r#""Infinity""#,
            r#""\u002d0""#,
        ] {
            assert_eq!(id(raw).unwrap(), raw, "{raw}");
        }
    }

    /// Literals that are not JSON numbers are refused by name, never coerced.
    #[test]
    fn invalid_number_literals_are_refused() {
        for raw in [
            "00", "+1", "NaN", "-", "1.", ".5", "1e", "1e+", "-01", "0x10", "Infinity",
        ] {
            let e = id(raw).unwrap_err().to_string();
            assert!(e.starts_with("_id "), "{raw}: {e}");
            assert!(!is_json_number(raw), "{raw}");
        }
        assert!(is_json_number("-0") && is_json_number("0e0") && is_json_number("10.25E-3"));
    }
}

#[cfg(test)]
mod target_index_outcome_tests {
    use super::target_index_outcome as t;

    #[test]
    fn unchanged_deleted_and_recreated() {
        assert_eq!(t(None, false, Ok(Some("a".into()))), None, "never noted");
        assert_eq!(t(None, false, Err(())), None, "never noted");
        assert_eq!(t(Some("a"), false, Ok(Some("a".into()))), None);
        assert_eq!(t(Some("a"), false, Err(())), Some(false));
        assert_eq!(t(Some("a"), false, Ok(Some("b".into()))), Some(true));
        // missing when the uuid was to be noted (deleted before the capture)
        assert_eq!(t(None, true, Err(())), Some(false));
        assert_eq!(t(None, true, Ok(Some("b".into()))), Some(true));
    }
}
