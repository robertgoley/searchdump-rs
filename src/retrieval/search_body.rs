use anyhow::Context;
use anyhow::Result;
use log::debug;
use sonic_rs::JsonContainerTrait;
use sonic_rs::JsonValueMutTrait;
use sonic_rs::JsonValueTrait;
use sonic_rs::{Value, json};
use tokio::fs;

/// elasticdump's default search body for ES >= 5.
pub fn default_search_body() -> Value {
    json!({"query": {"match_all": {}}, "stored_fields": ["*"], "_source": true})
}

/// Load `--searchBody`: inline JSON or `@path` (relative to the CWD).
pub async fn load_search_body(raw: Option<&str>) -> Result<Value> {
    let Some(raw) = raw else {
        debug!("No search body provided, using elasticdump's default match_all body");
        return Ok(default_search_body());
    };
    if let Some(file_path) = raw.strip_prefix('@') {
        debug!("Loading search body from file: {}", file_path);
        let content = fs::read_to_string(file_path)
            .await
            .with_context(|| format!("Failed to read search body from file: {}", file_path))?;
        sonic_rs::from_str(&content)
            .with_context(|| format!("Failed to parse search body JSON from file: {}", file_path))
    } else {
        sonic_rs::from_str(raw).context("Failed to parse search body JSON string")
    }
}

/// Normalise a search body and set its page `size`: `--limit`, or `--size`
/// when that is smaller (elasticdump's `min(size, limit)`).
pub fn finalize_search_body(body: Value, limit: usize, size: Option<u64>) -> Value {
    let mut final_search_body = if body.get_type() != sonic_rs::JsonType::Object {
        log::warn!(
            "searchBody is not a JSON object; using it as {{\"query\": {}}} — this is almost certainly not the intended query",
            sonic_rs::to_string(&body).unwrap_or_default()
        );
        json!({ "query": body })
    } else {
        body
    };

    let page_size = match size {
        Some(size) if size < limit as u64 => size.max(1),
        _ => limit as u64,
    };
    let obj = final_search_body.as_object_mut().unwrap();
    if let Some(user_size) = obj.get(&"size")
        && user_size.as_u64() != Some(page_size)
    {
        log::warn!(
            "Overriding searchBody size {} with --limit {}",
            sonic_rs::to_string(user_size).unwrap_or_default(),
            page_size
        );
    }
    obj.insert(&"size", json!(page_size));

    debug!(
        "Final search body: {}",
        sonic_rs::to_string(&final_search_body).unwrap_or_default()
    );
    final_search_body
}

/// Why `--scrollDocSort` cannot add `"sort":["_doc"]` to this scroll body, or `None` when it
/// can (1.1-3). A user sort is never touched, and a body whose hits carry meaningful relevance
/// scores keeps them: anything but a `match_all`/`match_none` query (or none), or
/// `track_scores`/`min_score`/`rescore` (Elasticsearch refuses `sort` with `rescore`).
pub fn doc_sort_blocker(body: &Value) -> Option<&'static str> {
    if body.get("sort").is_some() {
        return Some("the search body has its own sort");
    }
    if ["track_scores", "min_score", "rescore", "knn"]
        .iter()
        .any(|k| body.get(*k).is_some())
    {
        return Some("the search body uses scores (track_scores, min_score, rescore or knn)");
    }
    let unscored = match body.get("query") {
        None => true,
        Some(q) => q.as_object().is_some_and(|o| {
            o.len() == 1 && (o.get(&"match_all").is_some() || o.get(&"match_none").is_some())
        }),
    };
    (!unscored)
        .then_some("the query is not match_all/match_none, so its hits carry relevance scores")
}

/// Add `"sort":["_doc"]` (the cheapest scroll order) to a scroll body; see
/// [`doc_sort_blocker`] for when it may be used.
pub fn add_scroll_doc_sort(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(&"sort", json!(["_doc"]));
    }
}

/// `--searchBody` (or the default) with its page size applied.
pub async fn prepare_search_body(
    raw: Option<&str>,
    limit: usize,
    size: Option<u64>,
) -> Result<Value> {
    Ok(finalize_search_body(
        load_search_body(raw).await?,
        limit,
        size,
    ))
}

#[cfg(test)]
mod tests {
    use super::{add_scroll_doc_sort, doc_sort_blocker, prepare_search_body};
    use sonic_rs::json;

    // 1.1-3: `_doc` sort only for unscored bodies without their own sort.
    #[tokio::test]
    async fn scroll_doc_sort_is_added_only_where_scores_do_not_matter() {
        let mut body = prepare_search_body(None, 100, None).await.unwrap();
        assert_eq!(doc_sort_blocker(&body), None, "default match_all body");
        add_scroll_doc_sort(&mut body);
        assert_eq!(
            body,
            json!({"query": {"match_all": {}}, "stored_fields": ["*"], "_source": true, "size": 100, "sort": ["_doc"]})
        );
        for ok in [
            r#"{"size":5}"#,
            r#"{"query":{"match_all":{"boost":1.0}}}"#,
            r#"{"query":{"match_none":{}},"_source":["a"]}"#,
        ] {
            let b = prepare_search_body(Some(ok), 10, None).await.unwrap();
            assert_eq!(doc_sort_blocker(&b), None, "{ok}");
        }
        for (bad, why) in [
            (r#"{"sort":["ts"]}"#, "own sort"),
            (r#"{"sort":[]}"#, "own sort"),
            (r#"{"query":{"match":{"msg":"error"}}}"#, "relevance"),
            (
                r#"{"query":{"bool":{"filter":[{"term":{"k":1}}]}}}"#,
                "relevance",
            ),
            (
                r#"{"query":{"match_all":{}},"track_scores":true}"#,
                "scores",
            ),
            (r#"{"query":{"match_all":{}},"min_score":1}"#, "scores"),
            (r#"{"rescore":{"window_size":10}}"#, "scores"),
            (
                r#"{"knn":{"field":"v","query_vector":[1.0],"k":5,"num_candidates":10}}"#,
                "scores",
            ),
            (r#"{"query":{"match_all":{}},"sort":"_doc"}"#, "own sort"),
        ] {
            let b = prepare_search_body(Some(bad), 10, None).await.unwrap();
            let r = doc_sort_blocker(&b).unwrap_or_else(|| panic!("{bad} must be refused"));
            assert!(r.contains(why), "{bad}: {r}");
        }
    }

    #[tokio::test]
    async fn default_body_matches_elasticdump() {
        let body = prepare_search_body(None, 100, None).await.unwrap();
        assert_eq!(
            body,
            json!({"query": {"match_all": {}}, "stored_fields": ["*"], "_source": true, "size": 100})
        );
    }

    #[tokio::test]
    async fn warns_but_overrides_user_size() {
        let body = prepare_search_body(Some(r#"{"query":{"match_all":{}},"size":5}"#), 100, None)
            .await
            .unwrap();
        assert_eq!(body["size"], json!(100));
        assert_eq!(body["query"], json!({"match_all": {}}));
    }

    #[tokio::test]
    async fn smaller_total_size_shrinks_the_page() {
        let body = prepare_search_body(None, 100, Some(7)).await.unwrap();
        assert_eq!(body["size"], json!(7));
        let body = prepare_search_body(None, 100, Some(700)).await.unwrap();
        assert_eq!(body["size"], json!(100));
    }

    #[tokio::test]
    async fn file_parse_errors_include_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.json");
        std::fs::write(&path, "{not json").unwrap();
        let error = prepare_search_body(Some(&format!("@{}", path.display())), 100, None)
            .await
            .unwrap_err();
        let rendered = format!("{error:#}");
        assert!(rendered.contains(&path.display().to_string()), "{rendered}");
    }

    #[tokio::test]
    async fn wraps_non_object_search_body_in_query() {
        let body = prepare_search_body(Some(r#""bare-string""#), 10, None)
            .await
            .unwrap();
        assert_eq!(body["query"], json!("bare-string"));
        assert_eq!(body["size"], json!(10));
    }
}
