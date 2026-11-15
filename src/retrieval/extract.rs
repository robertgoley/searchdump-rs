use anyhow::{Result, anyhow};
use bytes::Bytes;
use sonic_rs::{JsonValueTrait, LazyValue, PointerTree, get_many, pointer};
use std::sync::OnceLock;

use crate::cli::SearchType;
use crate::pipeline::json_split::single_line_slice;

use super::retrieval_task::TotalHitsEstimate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BatchMetadata {
    pub(crate) total_hits: TotalHitsEstimate,
    pub(crate) next_scroll_id: Option<String>,
    pub(crate) next_pit_id: Option<String>,
    pub(crate) last_sort_raw: Option<Vec<u8>>,
    pub(crate) doc_count: u64,
    pub(crate) hits_are_empty: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ExtractedBatch {
    pub(crate) metadata: BatchMetadata,
    pub(crate) output: ExtractedOutputBatch,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ExtractedOutputBatch {
    pub(crate) buffer: Vec<u8>,
    pub(crate) doc_count: u64,
}

struct MetadataValues {
    relation: Option<String>,
    total_value: Option<u64>,
    next_pit_id: Option<String>,
    next_scroll_id: Option<String>,
    shards_failed: Option<u64>,
    timed_out: Option<bool>,
    /// `_shards.failures[0]` summarised (index, shard, reason type and text).
    shard_failure: Option<String>,
}

struct MetadataInputs<'a> {
    values: MetadataValues,
    hits: LazyValue<'a>,
}

fn metadata_tree() -> &'static PointerTree {
    static TREE: OnceLock<PointerTree> = OnceLock::new();

    TREE.get_or_init(|| {
        let mut tree = PointerTree::new();
        tree.add_path(&["_scroll_id"]);
        tree.add_path(&["pit_id"]);
        // `hits.total` as a whole: `{value, relation}` (ES 7+/OpenSearch) or a bare integer
        // (`rest_total_hits_as_int=true`, ES 6). Pointing into `.value` would make get_many
        // fail with a type error on the integer form (TST-API-013).
        tree.add_path(&["hits", "total"]);
        tree.add_path(&pointer!["hits", "hits"]);
        // Popped FIRST in extract_metadata_inputs (see the pop order there).
        tree.add_path(&["_shards", "failed"]);
        tree.add_path(&["timed_out"]);
        tree.add_path(&["_shards", "failures"]);
        tree
    })
}

/// `index[shard]: type: reason` of the first entry of `_shards.failures`,
/// truncated so a huge stack trace cannot flood the message.
fn summarize_shard_failure(raw: &str) -> Option<String> {
    let v: sonic_rs::Value = sonic_rs::from_str(raw).ok()?;
    let f = v.get(0)?;
    let index = f.get("index").and_then(|x| x.as_str()).unwrap_or("?");
    let shard = f
        .get("shard")
        .and_then(|x| x.as_i64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".into());
    let reason = f.get("reason");
    let ty = reason
        .and_then(|r| r.get("type"))
        .and_then(|x| x.as_str())
        .unwrap_or("unknown");
    let text = reason
        .and_then(|r| r.get("reason"))
        .and_then(|x| x.as_str())
        .unwrap_or("");
    let mut out = format!("{index}[{shard}]: {ty}: {text}");
    if out.len() > 512 {
        let mut cut = 512;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push('…');
    }
    Some(out)
}

fn require_hits_array(hits: Option<LazyValue<'_>>) -> Result<LazyValue<'_>> {
    let hits = hits.ok_or_else(|| anyhow!("Elasticsearch response is missing hits.hits"))?;
    if !hits.is_array() {
        return Err(anyhow!(
            "Elasticsearch response field hits.hits is not an array"
        ));
    }

    Ok(hits)
}

fn count_hits_and_last_sort_raw(
    hits: LazyValue<'_>,
    search_type: &SearchType,
) -> Result<(u64, Option<Vec<u8>>)> {
    let mut doc_count = 0u64;
    let mut last_sort_raw = None;
    let iter = hits
        .into_array_iter()
        .ok_or_else(|| anyhow!("Elasticsearch response field hits.hits is not iterable"))?;

    for hit in iter {
        let hit = hit?;
        doc_count += 1;

        if search_type.uses_sort() {
            match hit.get("sort") {
                Some(value) if value.is_array() => {
                    let buffer = last_sort_raw.get_or_insert_with(Vec::new);
                    buffer.clear();
                    buffer.extend_from_slice(value.as_raw_str().as_bytes());
                }
                _ => {
                    last_sort_raw = None;
                }
            }
        }
    }

    Ok((doc_count, last_sort_raw))
}

fn extract_metadata_inputs(response_bytes: &Bytes) -> Result<MetadataInputs<'_>> {
    let mut values = get_many(response_bytes, metadata_tree())?;
    // INVARIANT: get_many yields values in metadata_tree()'s add order, and pop()
    // returns them in reverse. metadata_tree() adds, in order: _scroll_id, pit_id,
    // hits.total, hits.hits, _shards.failed, timed_out, _shards.failures.
    // So these pops MUST stay in this exact (reverse) order and adjacent.
    let shard_failure = values
        .pop()
        .flatten()
        .and_then(|v| summarize_shard_failure(v.as_raw_str()));
    let timed_out = values.pop().flatten().and_then(|value| value.as_bool());
    let shards_failed = values.pop().flatten().and_then(|value| value.as_u64());
    let hits = require_hits_array(values.pop().flatten())?;
    let (total_value, relation) = match values.pop().flatten() {
        Some(total) if total.is_object() => (
            total.get("value").and_then(|v| v.as_u64()),
            total
                .get("relation")
                .and_then(|v| v.as_str().map(str::to_owned)),
        ),
        Some(total) => (total.as_u64(), None),
        None => (None, None),
    };
    let next_pit_id = values
        .pop()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_owned));
    let next_scroll_id = values
        .pop()
        .flatten()
        .and_then(|value| value.as_str().map(str::to_owned));

    Ok(MetadataInputs {
        values: MetadataValues {
            relation,
            total_value,
            next_pit_id,
            next_scroll_id,
            shards_failed,
            timed_out,
            shard_failure,
        },
        hits,
    })
}

fn finalize_metadata(
    values: MetadataValues,
    search_type: &SearchType,
    doc_count: u64,
    last_sort_raw: Option<Vec<u8>>,
) -> Result<BatchMetadata> {
    if search_type.uses_sort() && doc_count > 0 && last_sort_raw.is_none() {
        return Err(anyhow!(
            "search_after response contains hits but the final hit is missing a usable sort value"
        ));
    }

    // Partial responses must NEVER be treated as a complete batch: a timed-out or
    // failed-shard response silently drops documents. Absent fields (None) are fine
    // (not every response carries them) — only positive signals reject the batch.
    if values.timed_out == Some(true) {
        return Err(anyhow!(
            "Elasticsearch response reports timed_out=true; refusing partial results (raise --requestTimeout / cluster timeouts and retry)"
        ));
    }
    if let Some(failed) = values.shards_failed.filter(|f| *f > 0) {
        let why = values
            .shard_failure
            .map(|r| format!(" (first failure: {r})"))
            .unwrap_or_default();
        return Err(anyhow!(
            "Elasticsearch response reports {failed} failed shard(s){why}; refusing a partial dump (check cluster health and retry)"
        ));
    }

    let total_hits = match (values.total_value, values.relation.as_deref()) {
        (Some(value), Some("eq")) => TotalHitsEstimate {
            value,
            is_exact: true,
        },
        (Some(value), Some(_)) => TotalHitsEstimate {
            value,
            is_exact: false,
        },
        (Some(value), None) => TotalHitsEstimate {
            value,
            is_exact: true,
        },
        (None, _) => TotalHitsEstimate {
            value: doc_count,
            is_exact: false,
        },
    };

    Ok(BatchMetadata {
        total_hits,
        next_scroll_id: values.next_scroll_id,
        next_pit_id: values.next_pit_id,
        last_sort_raw,
        doc_count,
        hits_are_empty: doc_count == 0,
    })
}

/// Refuse a page nested deeper than any document the engines accept before the recursive
/// parse can overflow the stack (TST-BRK-IN-008); +8 covers the response's own levels.
fn check_depth(body: &Bytes) -> Result<()> {
    use crate::pipeline::bulk::{MAX_DOC_DEPTH, too_deep};
    match too_deep(body, MAX_DOC_DEPTH + 8) {
        Some(at) => Err(anyhow!(
            "the search response nests more than {MAX_DOC_DEPTH} levels deep (at byte {at}); nothing of this page was written"
        )),
        None => Ok(()),
    }
}

pub(crate) fn extract_batch_metadata(
    response_bytes: &Bytes,
    search_type: &SearchType,
) -> Result<BatchMetadata> {
    check_depth(response_bytes)?;
    let metadata_inputs = extract_metadata_inputs(response_bytes)?;
    let (doc_count, last_sort_raw) =
        count_hits_and_last_sort_raw(metadata_inputs.hits, search_type)?;

    finalize_metadata(
        metadata_inputs.values,
        search_type,
        doc_count,
        last_sort_raw,
    )
}

/// A hit without `_source` (TST-BRK-XV-002): either its index does not store it, or the
/// search body asked for `stored_fields` without `_source`, which makes the cluster leave
/// `_source` out (L-455a), or its `_source` filter leaves nothing (L-455b); the message
/// names whichever applies.
fn missing_source_error(hit: &LazyValue<'_>, body: super::BodySource) -> anyhow::Error {
    let index = hit
        .get("_index")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| "?".to_string());
    if body == super::BodySource::Filter {
        return anyhow!(
            "documents cannot be fully dumped: a hit from index '{index}' has no _source because the search body's _source filter leaves it out (only _id and metadata could be written). Nothing of this page was written; remove the _source filter from the search body to keep the documents, or pass --allowMissingSource to dump them without _source"
        );
    }
    if body == super::BodySource::StoredFields {
        return anyhow!(
            "documents cannot be fully dumped: a hit from index '{index}' has no _source because the search body sets stored_fields without _source, so the cluster leaves _source out (only _id, metadata and the stored fields could be written). Nothing of this page was written; add \"_source\": true to the search body to keep the documents, or pass --allowMissingSource to dump them without _source"
        );
    }
    anyhow!(
        "documents cannot be fully dumped: a hit from index '{index}' has no _source (its mapping has _source.enabled: false, so only _id and metadata could be written). Nothing of this page was written; pass --allowMissingSource to dump such indices anyway"
    )
}

/// `{}` with any whitespace inside (L-455c).
fn is_empty_object(raw: &[u8]) -> bool {
    let mut it = raw.iter().filter(|c| !c.is_ascii_whitespace());
    it.next() == Some(&b'{') && it.next() == Some(&b'}') && it.next().is_none()
}

/// A hit whose `_source` is `{}` (L-455c): with a `_source` filter in the search body that
/// is the likely cause; without one the document itself is empty in the source index, and
/// an empty document needs --allowMissingSource too. Names the hit's `_index` and `_id`.
fn empty_source_error(hit: &LazyValue<'_>, body: super::BodySource) -> anyhow::Error {
    let field = |k: &str| {
        let s: String = hit
            .get(k)
            .map(|v| {
                v.as_str()
                    .map_or_else(|| v.as_raw_str().to_string(), str::to_string)
            })
            .unwrap_or_else(|| "?".to_string())
            .chars()
            .take(200)
            .collect();
        s.escape_debug().to_string()
    };
    let (index, id) = (field("_index"), field("_id"));
    if body == super::BodySource::Filter {
        return anyhow!(
            "documents cannot be fully dumped: document '{id}' in index '{index}' has an empty _source ({{}}), most likely because the search body's _source filter leaves none of its fields (or the document is really empty in the source index). Nothing of this page was written; change the _source filter to keep the documents, or pass --allowMissingSource to dump them with an empty _source"
        );
    }
    anyhow!(
        "documents cannot be fully dumped: document '{id}' in index '{index}' has an empty _source ({{}}): the document is empty in the source index (or the index mapping's _source includes/excludes leave none of its fields). An empty document counts as one without _source; nothing of this page was written; pass --allowMissingSource to dump empty documents as they are"
    )
}

/// A response body that is not a search response (TST-BRK-NET-011): say what it looks like.
/// `obj` (one JSON object) without its top-level member `drop`. Every other member keeps its
/// raw value bytes (so `_source` stays byte for byte); keys are re-encoded, which is
/// byte-identical for the plain ASCII metadata keys of a hit (1.1-3).
fn without_member(obj: &[u8], drop: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(obj.len());
    out.push(b'{');
    for m in sonic_rs::to_object_iter(obj) {
        let (k, v) = m.map_err(|e| anyhow!("malformed hit: {e}"))?;
        if k == drop {
            continue;
        }
        if out.len() > 1 {
            out.push(b',');
        }
        sonic_rs::to_writer(&mut out, &*k)?;
        out.push(b':');
        out.extend_from_slice(v.as_raw_str().as_bytes());
    }
    out.push(b'}');
    Ok(out)
}

fn malformed(body: &Bytes, e: anyhow::Error) -> anyhow::Error {
    if body.starts_with(&[0x1f, 0x8b]) {
        return anyhow!(
            "the search response is gzip-compressed although compression was not requested (a proxy re-encoding responses?); pass --esCompress to accept compressed responses, or fix the proxy"
        );
    }
    let head: String = String::from_utf8_lossy(&body[..body.len().min(60)])
        .chars()
        .map(|c| if c.is_control() { '.' } else { c })
        .collect();
    e.context(format!(
        "malformed search response ({} bytes, starting {head:?}); nothing of this page was written",
        body.len()
    ))
}

pub(crate) fn extract_batch(
    response_bytes: &Bytes,
    search_type: &SearchType,
) -> Result<ExtractedBatch> {
    // PIT/search_after need `sort` to page; only the injected scroll `_doc` sort is dropped.
    let strip_sort = super::strip_hit_sort() && !search_type.uses_sort();
    extract_batch_with(response_bytes, search_type, strip_sort)
}

/// [`extract_batch`] with the `--scrollDocSort` strip decision passed in (testable without the
/// process-wide flag).
fn extract_batch_with(
    response_bytes: &Bytes,
    search_type: &SearchType,
    strip_sort: bool,
) -> Result<ExtractedBatch> {
    // sonic-rs does not validate escapes inside the values it hands out raw, so a
    // malformed response would become an invalid dump line (L-512 fuzz finding).
    crate::pipeline::bulk::check_escapes(response_bytes)
        .map_err(|e| anyhow!("malformed search response: {e}"))?;
    check_depth(response_bytes)?;
    let metadata_inputs =
        extract_metadata_inputs(response_bytes).map_err(|e| malformed(response_bytes, e))?;
    let iter = metadata_inputs
        .hits
        .into_array_iter()
        .ok_or_else(|| anyhow!("Elasticsearch response field hits.hits is not iterable"))?;

    let mut doc_count = 0u64;
    let mut last_sort_raw = None;
    let mut buffer = Vec::with_capacity(response_bytes.len());

    let require_source = super::require_source();
    let body_drops_source = super::body_drops_source();
    for hit in iter {
        let hit = hit.map_err(|e| malformed(response_bytes, e.into()))?;
        doc_count += 1;
        if require_source {
            match hit.get("_source") {
                None => return Err(missing_source_error(&hit, body_drops_source)),
                Some(s) if is_empty_object(s.as_raw_str().as_bytes()) => {
                    return Err(empty_source_error(&hit, body_drops_source));
                }
                Some(_) => {}
            }
        }

        // The cluster returns `_source` byte for byte as indexed, so a
        // pretty-printed document carries raw line breaks between tokens.
        // Every hit must be exactly one NDJSON line (INJ-F1).
        if strip_sort && hit.get("sort").is_some() {
            let trimmed = without_member(hit.as_raw_str().as_bytes(), "sort")
                .map_err(|e| malformed(response_bytes, e))?;
            buffer.extend_from_slice(&single_line_slice(&trimmed));
        } else {
            buffer.extend_from_slice(&single_line_slice(hit.as_raw_str().as_bytes()));
        }
        buffer.push(b'\n');

        if search_type.uses_sort() {
            match hit.get("sort") {
                Some(value) if value.is_array() => {
                    let sort_buffer = last_sort_raw.get_or_insert_with(Vec::new);
                    sort_buffer.clear();
                    sort_buffer.extend_from_slice(value.as_raw_str().as_bytes());
                }
                _ => {
                    last_sort_raw = None;
                }
            }
        }
    }

    let metadata = finalize_metadata(
        metadata_inputs.values,
        search_type,
        doc_count,
        last_sort_raw,
    )?;

    Ok(ExtractedBatch {
        metadata,
        output: ExtractedOutputBatch { buffer, doc_count },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        count_hits_and_last_sort_raw, empty_source_error, extract_batch, extract_batch_metadata,
        extract_batch_with, is_empty_object, missing_source_error, require_hits_array,
        without_member,
    };
    use crate::cli::SearchType;
    use bytes::Bytes;

    // L-455c: an empty _source ({}) is detected with any inner whitespace and named with
    // the hit's _index/_id (escaped) and the likely cause; a filter that excludes every
    // field is found before the first search
    #[test]
    fn empty_source_is_detected_and_named() {
        for raw in ["{}", "{ }", "{\n  }", " {\t} "] {
            assert!(is_empty_object(raw.as_bytes()), "{raw:?}");
        }
        for raw in ["{\"a\":1}", "[]", "\"{}\"", "{}{}", "null"] {
            assert!(!is_empty_object(raw.as_bytes()), "{raw:?}");
        }
        let hit = sonic_rs::from_str::<sonic_rs::LazyValue<'_>>(
            r#"{"_index":"i1","_id":"d\u001b9","_source":{}}"#,
        )
        .unwrap();
        let doc = empty_source_error(&hit, super::super::BodySource::Kept).to_string();
        assert!(
            doc.contains("document 'd\\u{1b}9' in index 'i1' has an empty _source ({})")
                && doc.contains("the document is empty in the source index")
                && doc.contains("--allowMissingSource")
                && !doc.contains("search body"),
            "{doc}"
        );
        let filt = empty_source_error(&hit, super::super::BodySource::Filter).to_string();
        assert!(
            filt.contains("the search body's _source filter leaves none of its fields")
                && filt.contains("--allowMissingSource"),
            "{filt}"
        );
        let all =
            |b: &str| super::super::source_filter_excludes_all(&sonic_rs::from_str(b).unwrap());
        assert_eq!(
            all(r#"{"_source":{"excludes":["x","*"]}}"#).as_deref(),
            Some("*")
        );
        assert_eq!(
            all(r#"{"_source":{"exclude":"**"}}"#).as_deref(),
            Some("**")
        );
        for keep in [
            r#"{"_source":{"excludes":["x*","*y"]}}"#,
            r#"{"_source":{"includes":["*"]}}"#,
            r#"{"_source":["*"]}"#,
            r#"{"_source":"*"}"#,
            r#"{"_source":{"excludes":[""]}}"#,
            r#"{"query":{"match_all":{}}}"#,
        ] {
            assert_eq!(all(keep), None, "{keep}");
        }
    }

    // L-455a: a hit without _source names the search body when it set stored_fields
    // without _source, and the mapping otherwise
    #[test]
    fn missing_source_names_the_search_body_or_the_mapping() {
        let hit =
            sonic_rs::from_str::<sonic_rs::LazyValue<'_>>(r#"{"_index":"i1","_id":"1"}"#).unwrap();
        let body = missing_source_error(&hit, super::super::BodySource::StoredFields).to_string();
        assert!(body.contains("'i1'"), "{body}");
        assert!(
            body.contains("the search body sets stored_fields without _source"),
            "{body}"
        );
        assert!(body.contains("\"_source\": true") && body.contains("--allowMissingSource"));
        assert!(!body.contains("_source.enabled"), "{body}");
        let mapping = missing_source_error(&hit, super::super::BodySource::Kept).to_string();
        let filter = missing_source_error(&hit, super::super::BodySource::Filter).to_string();
        assert!(
            filter.contains("the search body's _source filter")
                && filter.contains("--allowMissingSource")
                && !filter.contains("_source.enabled"),
            "{filter}"
        );
        assert!(
            mapping.contains("its mapping has _source.enabled: false"),
            "{mapping}"
        );
        assert!(!mapping.contains("stored_fields"), "{mapping}");
    }

    #[test]
    fn extract_batch_metadata_reads_scroll_total_and_scroll_id() {
        let response = Bytes::from_static(
            br#"{
                "_scroll_id":"scroll-123",
                "hits":{
                    "total":{"value":2,"relation":"eq"},
                    "hits":[
                        {"_id":"1","_source":{"name":"a"}},
                        {"_id":"2","_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let metadata = extract_batch_metadata(&response, &SearchType::Scroll).unwrap();

        assert_eq!(metadata.next_scroll_id.as_deref(), Some("scroll-123"));
        assert_eq!(metadata.total_hits.value, 2);
        assert!(metadata.total_hits.is_exact);
        assert_eq!(metadata.doc_count, 2);
        assert!(!metadata.hits_are_empty);
        assert!(metadata.last_sort_raw.is_none());
    }

    #[test]
    fn extract_batch_metadata_reads_pit_id_and_last_sort() {
        let response = Bytes::from_static(
            br#"{
                "pit_id":"pit-next",
                "hits":{
                    "total":{"value":10000,"relation":"gte"},
                    "hits":[
                        {"_id":"1","sort":[1,"a"],"_source":{"name":"a"}},
                        {"_id":"2","sort":[2,"b"],"_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let metadata = extract_batch_metadata(&response, &SearchType::PointInTime).unwrap();

        assert_eq!(metadata.next_pit_id.as_deref(), Some("pit-next"));
        assert_eq!(metadata.total_hits.value, 10000);
        assert!(!metadata.total_hits.is_exact);
        assert_eq!(
            metadata.last_sort_raw.as_deref(),
            Some(br#"[2,"b"]"#.as_slice())
        );
        assert_eq!(metadata.doc_count, 2);
    }

    #[test]
    fn count_hits_and_last_pit_sort_reads_raw_json_without_vec_per_hit() {
        let response = Bytes::from_static(
            br#"{
                "pit_id":"pit-next",
                "hits":{
                    "hits":[
                        {"_id":"1","sort":[1,"a"],"_source":{"name":"a"}},
                        {"_id":"2","sort":[2,{"nested":true}],"_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let hits =
            require_hits_array(Some(sonic_rs::get(&response, &["hits", "hits"]).unwrap())).unwrap();
        let (doc_count, last_sort_raw) =
            count_hits_and_last_sort_raw(hits, &SearchType::PointInTime).unwrap();

        assert_eq!(doc_count, 2);
        assert_eq!(
            last_sort_raw.as_deref(),
            Some(br#"[2,{"nested":true}]"#.as_slice())
        );
    }

    #[test]
    fn extract_batch_metadata_rejects_non_usable_final_pit_sort() {
        let response = Bytes::from_static(
            br#"{
                "pit_id":"pit-next",
                "hits":{
                    "total":{"value":2,"relation":"eq"},
                    "hits":[
                        {"_id":"1","sort":[1,"a"],"_source":{"name":"a"}},
                        {"_id":"2","sort":null,"_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let error = extract_batch_metadata(&response, &SearchType::PointInTime)
            .unwrap_err()
            .to_string();

        assert!(error.contains("usable sort value"));
    }

    #[test]
    fn extract_batch_metadata_reads_integer_total_as_exact() {
        // rest_total_hits_as_int=true / ES 6: used to fail with "invalid type: integer"
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","hits":{"total":7,"hits":[{"_id":"1","_source":{}}]}}"#,
        );
        let metadata = extract_batch_metadata(&response, &SearchType::Scroll).unwrap();
        assert_eq!(metadata.total_hits.value, 7);
        assert!(metadata.total_hits.is_exact);
        assert_eq!(metadata.next_scroll_id.as_deref(), Some("s"));
    }

    #[test]
    fn extract_batch_metadata_falls_back_to_hits_len_when_total_missing() {
        let response = Bytes::from_static(
            br#"{
                "_scroll_id":"scroll-123",
                "hits":{
                    "hits":[
                        {"_id":"1","_source":{"name":"a"}},
                        {"_id":"2","_source":{"name":"b"}},
                        {"_id":"3","_source":{"name":"c"}}
                    ]
                }
            }"#,
        );

        let metadata = extract_batch_metadata(&response, &SearchType::Scroll).unwrap();

        assert_eq!(metadata.total_hits.value, 3);
        assert!(!metadata.total_hits.is_exact);
        assert_eq!(metadata.doc_count, 3);
    }

    #[test]
    fn extract_batch_fuses_pit_metadata_and_output() {
        let response = Bytes::from_static(
            br#"{
                "pit_id":"pit-next",
                "hits":{
                    "total":{"value":2,"relation":"gte"},
                    "hits":[
                        {"_id":"1","sort":[1,"a"],"_source":{"name":"a"}},
                        {"_id":"2","sort":[2,{"nested":true}],"_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let extracted = extract_batch(&response, &SearchType::PointInTime).unwrap();

        assert_eq!(extracted.metadata.next_pit_id.as_deref(), Some("pit-next"));
        assert_eq!(extracted.metadata.total_hits.value, 2);
        assert!(!extracted.metadata.total_hits.is_exact);
        assert_eq!(
            extracted.metadata.last_sort_raw.as_deref(),
            Some(br#"[2,{"nested":true}]"#.as_slice())
        );
        assert_eq!(extracted.output.doc_count, 2);
        assert_eq!(
            String::from_utf8(extracted.output.buffer).unwrap(),
            concat!(
                "{\"_id\":\"1\",\"sort\":[1,\"a\"],\"_source\":{\"name\":\"a\"}}\n",
                "{\"_id\":\"2\",\"sort\":[2,{\"nested\":true}],\"_source\":{\"name\":\"b\"}}\n"
            )
        );
    }

    #[test]
    fn extract_batch_fuses_scroll_metadata_and_output() {
        let response = Bytes::from_static(
            br#"{
                "_scroll_id":"scroll-next",
                "hits":{
                    "total":{"value":2,"relation":"eq"},
                    "hits":[
                        {"_id":"1","_source":{"name":"a"}},
                        {"_id":"2","_source":{"name":"b"}}
                    ]
                }
            }"#,
        );

        let extracted = extract_batch(&response, &SearchType::Scroll).unwrap();

        assert_eq!(
            extracted.metadata.next_scroll_id.as_deref(),
            Some("scroll-next")
        );
        assert_eq!(extracted.metadata.total_hits.value, 2);
        assert!(extracted.metadata.total_hits.is_exact);
        assert!(extracted.metadata.last_sort_raw.is_none());
        assert_eq!(extracted.output.doc_count, 2);
        assert_eq!(
            String::from_utf8(extracted.output.buffer).unwrap(),
            concat!(
                "{\"_id\":\"1\",\"_source\":{\"name\":\"a\"}}\n",
                "{\"_id\":\"2\",\"_source\":{\"name\":\"b\"}}\n"
            )
        );
    }

    // 1.1-3: with --scrollDocSort the injected `sort` member is dropped from scroll hits;
    // `_source` stays byte for byte and `_score: null` is kept (documented difference).
    #[test]
    fn scroll_doc_sort_member_is_stripped_from_hits() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","hits":{"total":{"value":2,"relation":"eq"},"hits":[{"_index":"i","_id":"1","_score":null,"_source":{"n":1.0e2,"t":"\u00e9"},"sort":[0]},{"_index":"i","_id":"2","_score":null,"sort":[1],"_source":{"pretty":
 true}}]}}"#,
        );
        let out = extract_batch_with(&response, &SearchType::Scroll, true).unwrap();
        assert_eq!(out.output.doc_count, 2);
        assert_eq!(
            String::from_utf8(out.output.buffer).unwrap(),
            concat!(
                "{\"_index\":\"i\",\"_id\":\"1\",\"_score\":null,\"_source\":{\"n\":1.0e2,\"t\":\"\\u00e9\"}}\n",
                "{\"_index\":\"i\",\"_id\":\"2\",\"_score\":null,\"_source\":{\"pretty\":true}}\n"
            )
        );
        // off (the default): hits are written exactly as received
        let out = extract_batch_with(&response, &SearchType::Scroll, false).unwrap();
        assert!(
            String::from_utf8(out.output.buffer)
                .unwrap()
                .contains("\"sort\":[0]")
        );
        // a hit without sort is unchanged
        assert_eq!(
            without_member(br#"{"_id":"1","_source":{}}"#, "sort").unwrap(),
            br#"{"_id":"1","_source":{}}"#
        );
        assert_eq!(without_member(br#"{"sort":[1]}"#, "sort").unwrap(), b"{}");
        assert!(without_member(br#"{"_id":"1""#, "sort").is_err());
    }

    #[test]
    fn extract_batch_metadata_rejects_missing_hits_array() {
        let response = Bytes::from_static(br#"{"hits":{"total":{"value":0,"relation":"eq"}}}"#);

        let error = extract_batch_metadata(&response, &SearchType::Scroll)
            .unwrap_err()
            .to_string();

        assert!(error.contains("hits.hits"));
    }

    #[test]
    fn extract_batch_metadata_rejects_failed_shards() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","_shards":{"total":5,"successful":4,"skipped":0,"failed":1},"hits":{"total":{"value":10,"relation":"eq"},"hits":[{"_id":"1","_source":{}}]}}"#,
        );
        let error = extract_batch_metadata(&response, &SearchType::Scroll)
            .unwrap_err()
            .to_string();
        assert!(error.contains("1 failed shard"));
    }

    // L-428 (TST-API-052): the message names the shard reason; skipped shards are not failures.
    #[test]
    fn failed_shard_message_names_the_reason_and_skipped_is_fine() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","_shards":{"total":3,"successful":2,"skipped":0,"failed":1,"failures":[{"shard":2,"index":"logs-1","node":"n1","reason":{"type":"circuit_breaking_exception","reason":"[parent] Data too large"}}]},"hits":{"total":{"value":10,"relation":"eq"},"hits":[{"_id":"1","_source":{}}]}}"#,
        );
        let error = extract_batch_metadata(&response, &SearchType::Scroll)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("logs-1[2]: circuit_breaking_exception: [parent] Data too large"),
            "{error}"
        );
        let skipped = Bytes::from_static(
            br#"{"_scroll_id":"s","timed_out":false,"_shards":{"total":5,"successful":5,"skipped":3,"failed":0},"hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_id":"1","_source":{}}]}}"#,
        );
        assert!(extract_batch_metadata(&skipped, &SearchType::Scroll).is_ok());
        let long = format!(
            r#"[{{"shard":0,"index":"i","reason":{{"type":"t","reason":"{}"}}}}]"#,
            "x".repeat(5000)
        );
        assert!(
            super::summarize_shard_failure(&long)
                .unwrap()
                .chars()
                .count()
                <= 513
        );
    }

    #[test]
    fn extract_batch_metadata_rejects_timed_out_responses() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","timed_out":true,"_shards":{"total":5,"successful":5,"skipped":0,"failed":0},"hits":{"total":{"value":10,"relation":"eq"},"hits":[{"_id":"1","_source":{}}]}}"#,
        );
        let error = extract_batch_metadata(&response, &SearchType::Scroll)
            .unwrap_err()
            .to_string();
        assert!(error.contains("timed_out"));
    }

    #[test]
    fn extract_batch_metadata_accepts_zero_failed_shards() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"s","timed_out":false,"_shards":{"total":5,"successful":5,"skipped":0,"failed":0},"hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_id":"1","_source":{}}]}}"#,
        );

        let metadata = extract_batch_metadata(&response, &SearchType::Scroll).unwrap();

        assert_eq!(metadata.next_scroll_id.as_deref(), Some("s"));
        assert_eq!(metadata.doc_count, 1);
        assert!(!metadata.hits_are_empty);
    }

    #[test]
    fn extract_batch_preserves_bytes_with_escapes_and_unicode() {
        // Adversarial hits: JSON escapes (quote/backslash/slash), non-ASCII + emoji,
        // raw UTF-8, and _source keys colliding with metadata pointers
        // ("hits", "sort", "_scroll_id", "pit_id") carrying object/array values.
        let hit1 = r#"{"_id":"1","_source":{"text":"quote\"backslash\\slash\/","unicode":"é中😀","hits":{"nested":true},"sort":[1,"a"]}}"#;
        let hit2 = r#"{"_id":"2","_source":{"raw":"中文😀","_scroll_id":"decoy-scroll","pit_id":["decoy","array"]}}"#;
        let response_str = format!(
            r#"{{"_scroll_id":"real-scroll","hits":{{"total":{{"value":2,"relation":"eq"}},"hits":[{hit1},{hit2}]}}}}"#
        );
        let response = Bytes::from(response_str.into_bytes());

        let extracted = extract_batch(&response, &SearchType::Scroll).unwrap();

        // The top-level pointers must win over the colliding _source keys.
        assert_eq!(
            extracted.metadata.next_scroll_id.as_deref(),
            Some("real-scroll")
        );
        assert!(extracted.metadata.next_pit_id.is_none());

        // Byte-identical output: exact raw hit bytes joined by '\n', not a `contains` check.
        let mut expected = Vec::new();
        expected.extend_from_slice(hit1.as_bytes());
        expected.push(b'\n');
        expected.extend_from_slice(hit2.as_bytes());
        expected.push(b'\n');
        assert_eq!(extracted.output.buffer, expected);
        assert_eq!(extracted.output.doc_count, 2);
    }

    #[test]
    fn extract_batch_handles_empty_hits_array() {
        let response = Bytes::from_static(
            br#"{"_scroll_id":"scroll-empty","hits":{"total":{"value":0,"relation":"eq"},"hits":[]}}"#,
        );

        let extracted = extract_batch(&response, &SearchType::Scroll).unwrap();

        assert_eq!(extracted.metadata.doc_count, 0);
        assert!(extracted.metadata.hits_are_empty);
        assert!(extracted.output.buffer.is_empty());
        assert_eq!(extracted.output.doc_count, 0);
    }
}
