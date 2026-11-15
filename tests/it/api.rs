//! Phase A4 per-version API behaviour (TST-API-010 onwards). Every test runs on whatever
//! engine `scripts/it/up.sh` started; version-specific expectations are branched on
//! `Cluster::vnum()` / `engine()` so one source covers the whole matrix.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::harness::*;

pub(crate) fn io(c: &Cluster, input: &str, output: &str, extra: &[&str]) -> Vec<String> {
    let mut a = s(&["--input", input, "--output", output]);
    if input.starts_with("http") {
        a.extend(c.tls_args("input"));
    }
    if output.starts_with("http") {
        a.extend(c.tls_args("output"));
    }
    a.extend(s(extra));
    a
}

/// Dump `index` to a fresh file with `extra` flags; returns the file and the process output.
pub(crate) async fn dump(
    c: &Cluster,
    index: &str,
    extra: &[&str],
) -> (PathBuf, std::process::Output) {
    let out = tmp_path("api", "json");
    let o = run(io(c, &c.url_with_auth(index), out.to_str().unwrap(), extra)).await;
    (out, o)
}

/// As `dump`, without `--quiet` (for warning assertions).
pub(crate) async fn dump_loud(
    c: &Cluster,
    index: &str,
    extra: &[&str],
) -> (PathBuf, std::process::Output) {
    let out = tmp_path("api", "json");
    let o = run_loud(io(c, &c.url_with_auth(index), out.to_str().unwrap(), extra)).await;
    (out, o)
}

/// Same shape as `Cluster::docs`: id -> (routing, _source).
pub(crate) fn file_docs(path: &Path) -> BTreeMap<String, (Option<String>, Value)> {
    let mut m = BTreeMap::new();
    for l in lines(path) {
        let v: Value = serde_json::from_str(&l).unwrap();
        let routing = v["_routing"]
            .as_str()
            .or_else(|| v["fields"]["_routing"].as_str())
            .map(str::to_string);
        let id = v["_id"].as_str().unwrap().to_string();
        assert!(
            m.insert(id.clone(), (routing, v["_source"].clone()))
                .is_none(),
            "duplicate _id {id}"
        );
    }
    m
}

/// Open search contexts on one index (index-level stats, so concurrent tests on other
/// indices do not interfere): (scroll_current, point_in_time_current if the stat exists).
pub(crate) async fn open_contexts(c: &Cluster, index: &str) -> (u64, Option<u64>) {
    let v = c.ok("GET", &format!("{index}/_stats/search"), None).await;
    let s = &v["_all"]["total"]["search"];
    (
        s["scroll_current"].as_u64().unwrap_or(0),
        s["point_in_time_current"].as_u64(),
    )
}

/// Contexts are released asynchronously on some versions; poll briefly.
pub(crate) async fn assert_no_open_contexts(c: &Cluster, index: &str, what: &str) {
    let mut last = (0, None);
    for _ in 0..20 {
        last = open_contexts(c, index).await;
        if last.0 == 0 && last.1.unwrap_or(0) == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    panic!(
        "{what}: open contexts left on {index} (scroll_current, point_in_time_current) = {last:?}"
    );
}

// L-405 (TST-API-010): scroll initial / continuation / clear, unsliced and sliced with
// slices = shards and > shards; slices > index.max_slices_per_scroll is refused with the
// server's reason and nothing is written; no scroll contexts are left in any case.
#[tokio::test]
async fn scroll_unsliced_and_sliced_digest_and_no_open_contexts() {
    let c = Cluster::from_env();
    let src = unique("api_scroll");
    seed_index(&c, &src, 3000).await;
    let want = c.docs(&src).await;
    for slices in ["0", "2", "4", "7"] {
        let (f, o) = dump(
            &c,
            &src,
            &[
                "--slices",
                slices,
                "--sliceCap=false",
                "--limit",
                "250",
                "--scrollTime",
                "1m",
            ],
        )
        .await;
        assert_success(&o);
        assert_eq!(file_docs(&f), want, "slices={slices}");
        let _ = std::fs::remove_file(f);
        assert_no_open_contexts(&c, &src, &format!("slices={slices}")).await;
    }
    // over the per-index limit (default 1024): a clear failure carrying the server's reason
    let (f, o) = dump(&c, &src, &["--slices", "1100", "--sliceCap=false"]).await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("max_slices_per_scroll"), "{err}");
    assert!(!f.exists(), "nothing written on failure");
    assert_no_open_contexts(&c, &src, "slices=1100").await;
    c.delete_index(&src).await;
}

// L-406 (TST-API-011): PIT unsliced and sliced (2, 4, 16) give the source digest and leave no
// PIT open; versions without a PIT API (ES OSS, OpenSearch < 2.4) fall back to scroll with a
// warning naming the version. Request shapes (tiebreaker per version) are in the goldens.
#[tokio::test]
async fn pit_unsliced_and_sliced_digest_and_no_open_pits() {
    let c = Cluster::from_env();
    let src = unique("api_pit");
    seed_index(&c, &src, 2500).await;
    let want = c.docs(&src).await;
    let has_pit = c.has_pit().await;
    for slices in ["0", "2", "4", "16"] {
        let (f, o) = dump_loud(
            &c,
            &src,
            &[
                "--searchType",
                "pit",
                "--slices",
                slices,
                "--sliceCap=false",
                "--limit",
                "200",
            ],
        )
        .await;
        assert_success(&o);
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(
            err.contains("does not support point-in-time search"),
            !has_pit,
            "slices={slices}: fallback warning iff no PIT API: {err}"
        );
        assert_eq!(file_docs(&f), want, "slices={slices}");
        let _ = std::fs::remove_file(f);
        assert_no_open_contexts(&c, &src, &format!("pit slices={slices}")).await;
    }
    c.delete_index(&src).await;
}

// L-407 (TST-API-012): `--searchAfter` without a PIT needs a sort (usage error, exit 2); with a
// non-unique sort (1,000 docs sharing one timestamp) the tool used to stop after the first page
// and exit 0 with 100 of 1,000 documents. It now fails (exit 1, nothing written) naming the
// cause; with a unique tiebreaker it returns every document.
#[tokio::test]
async fn search_after_needs_sort_and_refuses_silent_loss_on_ties() {
    let c = Cluster::from_env();
    let src = unique("api_sa_ties");
    c.delete_index(&src).await;
    c.ok(
        "PUT",
        &src,
        Some(serde_json::json!({
            "settings": {"number_of_shards": 2, "number_of_replicas": 0},
            "mappings": {"properties": {"d": {"type": "date"}, "k": {"type": "keyword"}}}
        })),
    )
    .await;
    let mut nd = String::new();
    for i in 0..1000 {
        nd.push_str(&format!(
            "{{\"index\":{{\"_index\":\"{src}\",\"_id\":\"{i}\"}}}}\n"
        ));
        nd.push_str(&format!(
            "{{\"d\":\"2026-01-01T00:00:00Z\",\"k\":\"k{i:04}\"}}\n"
        ));
    }
    c.bulk_raw(nd).await;
    let want = c.docs(&src).await;

    let (f, o) = dump(&c, &src, &["--searchAfter"]).await;
    assert_eq!(
        o.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(!f.exists());

    let (f, o) = dump(
        &c,
        &src,
        &[
            "--searchAfter",
            "--limit",
            "100",
            "--searchBody",
            r#"{"sort":[{"d":"asc"}]}"#,
        ],
    )
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("retrieved 100 of 1000") && err.contains("not unique"),
        "{err}"
    );
    assert!(!f.exists(), "nothing committed on a shortfall");

    let (f, o) = dump(
        &c,
        &src,
        &[
            "--searchAfter",
            "--limit",
            "100",
            "--searchBody",
            r#"{"sort":[{"d":"asc"},{"k":"asc"}]}"#,
        ],
    )
    .await;
    assert_success(&o);
    assert_eq!(file_docs(&f), want);
    let _ = std::fs::remove_file(f);
    c.delete_index(&src).await;
}

// L-409 (TST-API-014): `--searchWithTemplate` with an inline and a stored (`id`) mustache
// template returns the same documents as the equivalent plain query.
#[tokio::test]
async fn search_templates_inline_and_stored_match_plain_query() {
    let c = Cluster::from_env();
    let src = unique("api_tpl");
    seed_index(&c, &src, 1200).await;
    let tid = unique("api_tpl_script");
    c.ok(
        "PUT",
        &format!("_scripts/{tid}"),
        Some(serde_json::json!({"script": {"lang": "mustache", "source": {"query": {"range": {"n": {"gte": "{{min}}"}}}}}})),
    )
    .await;
    let (plain, o) = dump(
        &c,
        &src,
        &["--searchBody", r#"{"query":{"range":{"n":{"gte":100}}}}"#],
    )
    .await;
    assert_success(&o);
    let want = file_docs(&plain);
    assert_eq!(want.len(), 1100);
    let inline = r#"{"source":{"query":{"range":{"n":{"gte":"{{min}}"}}}},"params":{"min":100}}"#;
    let stored = format!(r#"{{"id":"{tid}","params":{{"min":100}}}}"#);
    for (name, body) in [("inline", inline.to_string()), ("stored", stored)] {
        let (f, o) = dump(
            &c,
            &src,
            &[
                "--searchWithTemplate",
                "--searchBody",
                &body,
                "--limit",
                "300",
            ],
        )
        .await;
        assert_success(&o);
        assert_eq!(file_docs(&f), want, "{name}");
        let _ = std::fs::remove_file(f);
    }
    let _ = std::fs::remove_file(plain);
    let _ = c.call("DELETE", &format!("_scripts/{tid}"), None).await;
    c.delete_index(&src).await;
}

// L-410 (TST-API-015): `--searchBody` variants are passed through and their output shapes
// hold on every version; `size` is overridden by `--limit`; a user `sort` and a user `slice`
// are honoured; restore takes routing from `fields._routing` (meta order `X`, `_X`,
// `fields.X`, `fields._X`, unit-tested in pipeline::bulk).
#[tokio::test]
async fn search_body_variants_and_meta_from_fields() {
    let c = Cluster::from_env();
    let src = unique("api_sbv");
    c.delete_index(&src).await;
    c.ok(
        "PUT",
        &src,
        Some(serde_json::json!({
            "settings": {"number_of_shards": 2, "number_of_replicas": 0},
            "mappings": {"properties": {"n": {"type": "long"}, "k": {"type": "keyword"}, "x": {"type": "keyword", "store": true}}}
        })),
    )
    .await;
    let mut nd = String::new();
    for i in 0..300 {
        nd.push_str(&format!(
            "{{\"index\":{{\"_index\":\"{src}\",\"_id\":\"{i}\",\"routing\":\"r{}\"}}}}\n",
            i % 3
        ));
        nd.push_str(&format!(
            "{{\"n\":{i},\"k\":\"k{i}\",\"x\":\"x{i}\",\"y\":{{\"z\":{i}}}}}\n"
        ));
    }
    c.bulk_raw(nd).await;
    c.ok("POST", &format!("{src}/_refresh"), None).await;
    let parsed = |f: &Path| -> Vec<Value> {
        lines(f)
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    };
    let check = |name: &str, docs: &[Value], pred: &dyn Fn(&Value) -> bool| {
        assert_eq!(docs.len(), 300, "{name}");
        for d in docs {
            assert!(pred(d), "{name}: {d}");
            assert!(d["_routing"].is_string(), "{name}: routing kept: {d}");
        }
    };
    type Pred = Box<dyn Fn(&Value) -> bool>;
    let variants: Vec<(&str, &str, Pred)> = vec![
        (
            "includes",
            r#"{"_source":{"includes":["n"]}}"#,
            Box::new(|d| d["_source"].as_object().unwrap().keys().eq(["n"].iter())),
        ),
        (
            "excludes",
            r#"{"_source":{"excludes":["y"]}}"#,
            Box::new(|d| d["_source"].get("y").is_none() && d["_source"]["k"].is_string()),
        ),
        (
            "stored_fields",
            r#"{"stored_fields":["x"]}"#,
            Box::new(|d| d["fields"]["x"][0].is_string() && d.get("_source").is_none()),
        ),
        (
            "docvalue_fields",
            r#"{"docvalue_fields":["k"]}"#,
            Box::new(|d| d["fields"]["k"][0] == d["_source"]["k"]),
        ),
        (
            "fields",
            r#"{"fields":["k"],"_source":false}"#,
            Box::new(|d| d["fields"]["k"][0].is_string() && d.get("_source").is_none()),
        ),
        (
            "size override",
            r#"{"query":{"match_all":{}},"size":5}"#,
            Box::new(|d| d["_source"]["n"].is_u64()),
        ),
    ];
    // L-455b: `"_source": []` and `{"excludes":["*"]}` do not leave _source out of the hits
    // (checked on OpenSearch 1.3.20/2.19.7/3.9.0 and Elasticsearch 7.17/8.6/8.19/9.1: the
    // first returns the whole _source, the second `{}`). `"_source": []` is not refused.
    // L-455c (behaviour change): `{"excludes":["*"]}` used to be allowed and wrote `{}`; it
    // is now refused before the first search, and only --allowMissingSource writes `{}`.
    // `"_source": false` is refused too (the "fields" variant).
    for (body, whole) in [
        (r#"{"_source":[]}"#, true),
        (r#"{"_source":{"excludes":["*"]}}"#, false),
    ] {
        let mut b: Value = serde_json::from_str(body).unwrap();
        b["size"] = 1.into();
        let v = c.ok("POST", &format!("{src}/_search"), Some(b)).await;
        let hit = &v["hits"]["hits"][0];
        assert!(hit["_source"].is_object(), "{body}: {hit}");
        let mut args = vec!["--limit", "50", "--searchBody", body];
        if !whole {
            let (f, o) = dump(&c, &src, &args).await;
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            assert_eq!(o.status.code(), Some(1), "{body}: {err}");
            assert!(
                err.contains("the search body's _source filter excludes every field")
                    && err.contains("--allowMissingSource"),
                "{body}: {err}"
            );
            assert!(
                !f.exists() || lines(&f).is_empty(),
                "{body}: nothing written"
            );
            let _ = std::fs::remove_file(f);
            args.push("--allowMissingSource");
        }
        let (f, o) = dump(&c, &src, &args).await;
        assert_success(&o);
        let docs = parsed(&f);
        assert_eq!(docs.len(), 300, "{body}");
        for d in &docs {
            let n = d["_source"].as_object().map(|m| m.len());
            assert_eq!(n, Some(if whole { 4 } else { 0 }), "{body}: {d}");
        }
        let _ = std::fs::remove_file(f);
    }
    // L-455c: a document that is really empty in the source index (`{}`) stops the dump
    // naming its _id, without any search body; --allowMissingSource dumps it as it is.
    {
        let empty_idx = unique("api_sbv_empty");
        c.delete_index(&empty_idx).await;
        c.ok(
            "PUT",
            &format!("{empty_idx}/_doc/full?refresh=true"),
            Some(serde_json::json!({"n": 1})),
        )
        .await;
        c.ok(
            "PUT",
            &format!("{empty_idx}/_doc/hollow?refresh=true"),
            Some(serde_json::json!({})),
        )
        .await;
        let (f, o) = dump(&c, &empty_idx, &["--limit", "50"]).await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains("document 'hollow'")
                && err.contains("has an empty _source ({})")
                && err.contains("the document is empty in the source index")
                && err.contains("--allowMissingSource"),
            "{err}"
        );
        let _ = std::fs::remove_file(f);
        let (f, o) = dump(&c, &empty_idx, &["--limit", "50", "--allowMissingSource"]).await;
        assert_success(&o);
        let docs = parsed(&f);
        assert_eq!(docs.len(), 2);
        assert!(
            docs.iter()
                .any(|d| d["_id"] == "hollow" && d["_source"] == serde_json::json!({}))
        );
        let _ = std::fs::remove_file(f);
        c.delete_index(&empty_idx).await;
    }
    for (name, body, pred) in &variants {
        // L-455a: stored_fields without _source leaves _source out, which is refused by
        // default (naming the search body); L-455b: so is "_source": false (behaviour
        // change; it was allowed before). These variants then opt in
        let mut args = vec!["--limit", "50", "--searchBody", body];
        let refusal = match *name {
            "stored_fields" => Some("the search body sets stored_fields without _source"),
            "fields" => Some("the search body sets \"_source\": false"),
            _ => None,
        };
        if let Some(why) = refusal {
            let (f, o) = dump(&c, &src, &args).await;
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            assert_eq!(o.status.code(), Some(1), "{name}: {err}");
            assert!(
                err.contains(why) && err.contains("--allowMissingSource"),
                "{name}: {err}"
            );
            assert!(
                !f.exists() || lines(&f).is_empty(),
                "{name}: nothing written"
            );
            let _ = std::fs::remove_file(f);
            args.push("--allowMissingSource");
        }
        let (f, o) = dump(&c, &src, &args).await;
        assert_success(&o);
        check(name, &parsed(&f), pred.as_ref());
        let _ = std::fs::remove_file(f);
    }
    // user sort is kept (descending n)
    let (f, o) = dump(
        &c,
        &src,
        &[
            "--limit",
            "50",
            "--searchBody",
            r#"{"sort":[{"n":"desc"}]}"#,
        ],
    )
    .await;
    assert_success(&o);
    let ns: Vec<u64> = parsed(&f)
        .iter()
        .map(|d| d["_source"]["n"].as_u64().unwrap())
        .collect();
    assert_eq!(ns, (0..300).rev().collect::<Vec<_>>(), "user sort");
    let _ = std::fs::remove_file(f);
    // a user slice is honoured: same ids as the server's own sliced search
    let sliced = r#"{"query":{"match_all":{}},"slice":{"id":0,"max":2}}"#;
    let (f, o) = dump(&c, &src, &["--limit", "50", "--searchBody", sliced]).await;
    assert_success(&o);
    let got: std::collections::BTreeSet<String> = parsed(&f)
        .iter()
        .map(|d| d["_id"].as_str().unwrap().to_string())
        .collect();
    let v = c
        .ok(
            "POST",
            &format!("{src}/_search?scroll=1m"),
            Some(serde_json::json!({"size": 1000, "slice": {"id": 0, "max": 2}, "_source": false})),
        )
        .await;
    let want: std::collections::BTreeSet<String> = v["hits"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["_id"].as_str().unwrap().to_string())
        .collect();
    if let Some(sid) = v["_scroll_id"].as_str() {
        let _ = c
            .call(
                "DELETE",
                "_search/scroll",
                Some(serde_json::json!({"scroll_id": [sid]})),
            )
            .await;
    }
    assert!(!got.is_empty() && got.len() < 300);
    assert_eq!(got, want, "user slice");
    let _ = std::fs::remove_file(f);
    // restore: routing only in fields._routing (array form) is applied
    let dst = unique("api_sbv_dst");
    c.delete_index(&dst).await;
    c.ok(
        "PUT",
        &dst,
        Some(serde_json::json!({"settings": {"number_of_shards": 2, "number_of_replicas": 0}})),
    )
    .await;
    let file = tmp_path("api_fields_routing", "json");
    std::fs::write(&file, "{\"_index\":\"x\",\"_id\":\"a\",\"fields\":{\"_routing\":[\"rq\"]},\"_source\":{\"v\":1}}\n").unwrap();
    let o = run(io(&c, file.to_str().unwrap(), &c.url_with_auth(&dst), &[])).await;
    assert_success(&o);
    let (st, d) = c
        .call("GET", &format!("{dst}/_doc/a?routing=rq"), None)
        .await;
    assert_eq!(st, 200, "{d}");
    assert_eq!(d["_routing"], "rq", "{d}");
    let _ = std::fs::remove_file(file);
    c.delete_index(&src).await;
    c.delete_index(&dst).await;
}

fn report_counts(o: &std::process::Output) -> (u64, u64) {
    let err = String::from_utf8_lossy(&o.stderr);
    let num = |key: &str| {
        err.lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix(key)
                    .map(|v| v.trim().parse::<u64>().unwrap())
            })
            .unwrap_or(0)
    };
    (num("Total Writes:"), num("Failed Writes:"))
}

// L-412 (TST-API-021): `Total Writes` / `Failed Writes` equal the server's truth (`_count`)
// for mixed bulk responses: mapper_parsing_exception, version_conflict_engine_exception
// (create on an existing id), document_missing_exception (update of a missing id) and
// cluster_block_exception (index.blocks.write).
#[tokio::test]
async fn bulk_item_errors_are_counted_like_the_server() {
    let c = Cluster::from_env();
    let dst = unique("api_bulkerr");
    c.delete_index(&dst).await;
    c.ok(
        "PUT",
        &dst,
        Some(serde_json::json!({"settings": {"number_of_shards": 1, "number_of_replicas": 0}, "mappings": {"properties": {"n": {"type": "long"}}}})),
    )
    .await;
    c.ok(
        "PUT",
        &format!("{dst}/_doc/e1?refresh=true"),
        Some(serde_json::json!({"n": 0})),
    )
    .await;
    let mut input = String::new();
    for i in 0..10 {
        input.push_str(&format!(
            "{{\"_index\":\"x\",\"_id\":\"g{i}\",\"_source\":{{\"n\":{i}}}}}\n"
        ));
    }
    for i in 0..3 {
        input.push_str(&format!(
            "{{\"_index\":\"x\",\"_id\":\"m{i}\",\"_source\":{{\"n\":\"abc\"}}}}\n"
        ));
    }
    input.push_str("{\"_index\":\"x\",\"_id\":\"e1\",\"_source\":{\"n\":5}}\n");
    let file = tmp_path("api_bulkerr", "json");
    std::fs::write(&file, &input).unwrap();
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(&dst),
        &["--bulkAction", "create", "--limit", "5"],
    ))
    .await;
    let (written, failed) = report_counts(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    // ES >= 8.8 says document_parsing_exception where older versions say mapper_parsing_exception
    assert!(
        (err.contains("mapper_parsing_exception") || err.contains("document_parsing_exception"))
            && err.contains("version_conflict_engine_exception"),
        "{err}"
    );
    assert_eq!((written, failed), (10, 4), "{err}");
    assert_eq!(c.count(&dst).await, 1 + written);

    // update of missing ids: document_missing_exception, nothing written
    let missing = tmp_path("api_bulkerr_upd", "json");
    std::fs::write(&missing, "{\"_index\":\"x\",\"_id\":\"nope1\",\"_source\":{\"n\":1}}\n{\"_index\":\"x\",\"_id\":\"g1\",\"_source\":{\"n\":100}}\n").unwrap();
    let o = run_loud(io(
        &c,
        missing.to_str().unwrap(),
        &c.url_with_auth(&dst),
        &["--bulkAction", "update"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("document_missing_exception"), "{err}");
    assert_eq!(report_counts(&o), (1, 1), "{err}");
    assert_eq!(c.count(&dst).await, 11);

    // write block: every item fails with cluster_block_exception
    c.ok(
        "PUT",
        &format!("{dst}/_settings"),
        Some(serde_json::json!({"index.blocks.write": true})),
    )
    .await;
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(&dst),
        &["--limit", "5"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains("cluster_block_exception"), "{err}");
    assert_eq!(report_counts(&o), (0, 14), "{err}");
    c.ok(
        "PUT",
        &format!("{dst}/_settings"),
        Some(serde_json::json!({"index.blocks.write": false})),
    )
    .await;
    assert_eq!(c.count(&dst).await, 11);
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(missing);
    c.delete_index(&dst).await;
}

/// `GET <index>/_mapping` body without the index name.
async fn mapping_of(c: &Cluster, index: &str) -> Value {
    let v = c.ok("GET", &format!("{index}/_mapping"), None).await;
    v[index]["mappings"].clone()
}

// L-414 (TST-API-030): same-engine mapping round trips (dump `--type mapping` to a file,
// restore into a new index) are lossless for each mapping feature the version supports:
// a field literally named `properties`, runtime fields (ES >= 7.11), `flattened` (ES default
// distribution) / `flat_object` (OS >= 2.7), `dense_vector` / `knn_vector`, `_source`
// disabled, synthetic `_source` (ES >= 8.4 where the licence allows it), `subobjects: false`
// (ES >= 8.3). Cross-engine conversion is the expectation table (L-438/L-439).
#[tokio::test]
async fn mapping_round_trips_are_lossless_per_feature() {
    use serde_json::json;
    let c = Cluster::from_env();
    let v = c.vnum().await;
    let es = matches!(c.engine, Engine::Elasticsearch);
    let oss = c.is_oss().await;
    let mut variants: Vec<(&str, Value)> = vec![
        (
            "base",
            json!({"properties": {"k": {"type": "keyword"}, "n": {"type": "long"}, "obj": {"properties": {"properties": {"type": "keyword"}, "x": {"type": "integer"}}}}}),
        ),
        (
            "source_disabled",
            json!({"_source": {"enabled": false}, "properties": {"k": {"type": "keyword"}}}),
        ),
    ];
    if es && v >= (7, 11) {
        variants.push(("runtime", json!({"runtime": {"rt": {"type": "keyword", "script": {"source": "emit('x')"}}}, "properties": {"k": {"type": "keyword"}}})));
    }
    if es && !oss {
        variants.push((
            "flattened",
            json!({"properties": {"fl": {"type": "flattened"}}}),
        ));
        variants.push((
            "dense_vector",
            json!({"properties": {"dv": {"type": "dense_vector", "dims": 3}}}),
        ));
    }
    if !es && v >= (2, 7) {
        variants.push((
            "flat_object",
            json!({"properties": {"fo": {"type": "flat_object"}}}),
        ));
    }
    if !es {
        variants.push((
            "knn_vector",
            json!({"properties": {"kv": {"type": "knn_vector", "dimension": 3}}}),
        ));
    }
    if es && v >= (8, 3) {
        variants.push((
            "subobjects_false",
            json!({"subobjects": false, "properties": {"a.b": {"type": "keyword"}}}),
        ));
    }
    if es && v >= (8, 4) {
        variants.push((
            "synthetic_source",
            json!({"_source": {"mode": "synthetic"}, "properties": {"k": {"type": "keyword"}}}),
        ));
    }
    let mut done = vec![];
    for (name, mapping) in variants {
        let src = unique(&format!("api_map_{name}"));
        let dst = format!("{src}_dst");
        c.delete_index(&src).await;
        c.delete_index(&dst).await;
        let (st, body) = c
            .call(
                "PUT",
                &src,
                Some(json!({"settings": {"number_of_replicas": 0}, "mappings": mapping})),
            )
            .await;
        if st != 200 {
            // e.g. synthetic _source needs an enterprise licence on ES >= 8.17
            println!("SKIP: mapping variant {name} not accepted by this version: {body}");
            continue;
        }
        let file = tmp_path(&format!("api_map_{name}"), "json");
        let o = run(io(
            &c,
            &c.url_with_auth(&src),
            file.to_str().unwrap(),
            &["--type", "mapping"],
        ))
        .await;
        assert_success(&o);
        let o = run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&dst),
            &["--type", "mapping"],
        ))
        .await;
        assert_success(&o);
        assert_eq!(
            mapping_of(&c, &dst).await,
            mapping_of(&c, &src).await,
            "{name}"
        );
        let _ = std::fs::remove_file(file);
        c.delete_index(&src).await;
        c.delete_index(&dst).await;
        done.push(name);
    }
    println!("mapping round trips: {done:?}");
    assert!(done.contains(&"base") && done.contains(&"source_disabled"));
}

/// `index` settings of `index` with the keys every new index generates removed.
async fn settings_of(c: &Cluster, index: &str) -> Value {
    let v = c.ok("GET", &format!("{index}/_settings"), None).await;
    let mut s = v[index]["settings"]["index"].clone();
    if let Some(m) = s.as_object_mut() {
        for k in [
            "uuid",
            "creation_date",
            "provided_name",
            "version",
            "history",
            "resize",
        ] {
            m.remove(k);
        }
    }
    s
}

// L-415 (TST-API-031): `--type settings` dumps strip generated keys; restoring them into a
// new index reproduces the source settings (analysis included) modulo the generated-key
// allow-list; `--excludeSettings` drops the named keys (with or without `index.`).
#[tokio::test]
async fn settings_round_trip_strips_generated_keys_and_honours_exclude() {
    use serde_json::json;
    let c = Cluster::from_env();
    let src = unique("api_set");
    c.delete_index(&src).await;
    c.ok(
        "PUT",
        &src,
        Some(json!({"settings": {
            "number_of_shards": 2, "number_of_replicas": 0, "refresh_interval": "5s", "max_result_window": 20000,
            "analysis": {"analyzer": {"a1": {"type": "custom", "tokenizer": "whitespace", "filter": ["lowercase"]}},
                         "normalizer": {"lc": {"type": "custom", "filter": ["lowercase"]}}}
        }})),
    )
    .await;
    let file = tmp_path("api_set", "json");
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&src),
            file.to_str().unwrap(),
            &["--type", "settings"],
        ))
        .await,
    );
    let dumped = std::fs::read_to_string(&file).unwrap();
    for k in [
        "\\\"uuid\\\"",
        "creation_date",
        "provided_name",
        "\\\"version\\\"",
    ] {
        assert!(
            !dumped.contains(k),
            "generated key {k} in the dump: {dumped}"
        );
    }
    let dst = format!("{src}_dst");
    c.delete_index(&dst).await;
    assert_success(
        &run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&dst),
            &["--type", "settings"],
        ))
        .await,
    );
    assert_eq!(settings_of(&c, &dst).await, settings_of(&c, &src).await);

    let dst2 = format!("{src}_dst2");
    c.delete_index(&dst2).await;
    let o = run(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(&dst2),
        &[
            "--type",
            "settings",
            "--excludeSettings",
            "refresh_interval,index.max_result_window",
        ],
    ))
    .await;
    assert_success(&o);
    let mut want = settings_of(&c, &src).await;
    want.as_object_mut().unwrap().remove("refresh_interval");
    want.as_object_mut().unwrap().remove("max_result_window");
    assert_eq!(settings_of(&c, &dst2).await, want);
    let _ = std::fs::remove_file(file);
    for i in [&src, &dst, &dst2] {
        c.delete_index(i).await;
    }
}

/// Index names in a `--type index|settings|mapping` dump file (the document may be a
/// JSON-encoded string, as elasticdump writes it).
fn dumped_index_names(path: &Path) -> Vec<String> {
    let mut out = vec![];
    for l in lines(path) {
        let mut v: Value = serde_json::from_str(&l).unwrap();
        if let Some(s) = v.as_str() {
            v = serde_json::from_str(s).unwrap();
        }
        out.extend(v.as_object().into_iter().flat_map(|o| o.keys().cloned()));
    }
    out.sort();
    out
}

// L-416 (TST-API-032): `--type index` over a wildcard includes open visible indices with
// their aliases (filter, routing); hidden and closed indices are excluded (the server's
// default `expand_wildcards=open`), a hidden index named explicitly is included, and data
// stream backing indices are skipped with a warning naming the data stream.
#[tokio::test]
async fn index_type_inclusion_hidden_closed_and_data_streams() {
    use serde_json::json;
    let c = Cluster::from_env();
    let v = c.vnum().await;
    let es = matches!(c.engine, Engine::Elasticsearch);
    let p = unique("api_incl");
    let (vis, hid, clo, ds) = (
        format!("{p}_a"),
        format!("{p}_h"),
        format!("{p}_c"),
        format!("{p}_ds"),
    );
    c.ok("PUT", &vis, Some(json!({"settings": {"number_of_replicas": 0}, "aliases": {format!("{p}_al"): {"filter": {"term": {"k": "x"}}, "routing": "r1"}}}))).await;
    c.ok(
        "PUT",
        &hid,
        Some(json!({"settings": {"number_of_replicas": 0, "index.hidden": true}})),
    )
    .await;
    c.ok(
        "PUT",
        &clo,
        Some(json!({"settings": {"number_of_replicas": 0}})),
    )
    .await;
    c.ok("POST", &format!("{clo}/_close"), None).await;
    let tpl = format!("{p}_dstpl");
    let has_ds = if es {
        v >= (7, 9) && !c.is_oss().await
    } else {
        true
    };
    if has_ds {
        c.ok("PUT", &format!("_index_template/{tpl}"), Some(json!({"index_patterns": [format!("{ds}*")], "data_stream": {}, "priority": 500, "template": {"settings": {"number_of_replicas": 0}}}))).await;
        c.ok("PUT", &format!("_data_stream/{ds}"), None).await;
    }
    let file = tmp_path("api_incl", "json");
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&format!("{p}_*")),
        file.to_str().unwrap(),
        &["--type", "index"],
    ))
    .await;
    assert_success(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(dumped_index_names(&file), vec![vis.clone()], "{err}");
    if has_ds {
        assert!(
            err.contains("skipping data stream backing index") && err.contains(&ds),
            "{err}"
        );
    }
    let doc: Value = {
        let l = &lines(&file)[0];
        let v: Value = serde_json::from_str(l).unwrap();
        v.as_str()
            .map(|s| serde_json::from_str(s).unwrap())
            .unwrap_or(v)
    };
    let al = &doc[&vis]["aliases"][format!("{p}_al")];
    assert_eq!(al["filter"]["term"]["k"], "x", "{doc}");
    assert_eq!(al["index_routing"], "r1", "{doc}");
    let _ = std::fs::remove_file(&file);
    // a hidden index named explicitly is dumped
    let o = run(io(
        &c,
        &c.url_with_auth(&hid),
        file.to_str().unwrap(),
        &["--type", "index"],
    ))
    .await;
    assert_success(&o);
    assert_eq!(dumped_index_names(&file), vec![hid.clone()]);
    let _ = std::fs::remove_file(&file);
    if has_ds {
        let _ = c.call("DELETE", &format!("_data_stream/{ds}"), None).await;
        let _ = c
            .call("DELETE", &format!("_index_template/{tpl}"), None)
            .await;
    }
    for i in [&vis, &hid, &clo] {
        c.delete_index(i).await;
    }
}

async fn index_status(c: &Cluster, index: &str) -> String {
    let v = c.ok("GET", &format!("_cluster/state/metadata/{index}?filter_path=metadata.indices.*.state&expand_wildcards=all"), None).await;
    v["metadata"]["indices"][index]["state"]
        .as_str()
        .unwrap_or("?")
        .to_string()
}

// L-417 (TST-API-033): `--type analyzer` creates a new index; into an existing index it fails
// naming --closeIndex/--skip-existing; --skip-existing leaves it alone; --closeIndex closes,
// updates and reopens it; an update the server rejects (unknown tokenizer, injected) still
// reopens the index and exits 1.
#[tokio::test]
async fn analyzer_restore_never_leaves_the_index_closed() {
    use serde_json::json;
    let c = Cluster::from_env();
    let p = unique("api_an");
    let src = format!("{p}_src");
    c.ok("PUT", &src, Some(json!({"settings": {"number_of_replicas": 0, "analysis": {"analyzer": {"a1": {"type": "custom", "tokenizer": "whitespace", "filter": ["lowercase"]}}}}}))).await;
    let file = tmp_path("api_an", "json");
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&src),
            file.to_str().unwrap(),
            &["--type", "analyzer"],
        ))
        .await,
    );
    let good = std::fs::read_to_string(&file).unwrap();
    let analyzer = |idx: &str| {
        let idx = idx.to_string();
        let c = &c;
        async move {
            let v = c.ok("GET", &format!("{idx}/_settings"), None).await;
            v[&idx]["settings"]["index"]["analysis"]["analyzer"]["a1"]["tokenizer"].clone()
        }
    };
    // new index
    let fresh = format!("{p}_new");
    assert_success(
        &run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&fresh),
            &["--type", "analyzer"],
        ))
        .await,
    );
    assert_eq!(analyzer(&fresh).await, "whitespace");
    // existing index: clear error, then --skip-existing, then --closeIndex
    let ex = format!("{p}_ex");
    c.ok(
        "PUT",
        &ex,
        Some(json!({"settings": {"number_of_replicas": 0}})),
    )
    .await;
    let o = run(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(&ex),
        &["--type", "analyzer"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("--closeIndex") && err.contains("--skip-existing"),
        "{err}"
    );
    assert_eq!(index_status(&c, &ex).await, "open");
    assert_success(
        &run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&ex),
            &["--type", "analyzer", "--skip-existing"],
        ))
        .await,
    );
    assert!(
        analyzer(&ex).await.is_null(),
        "--skip-existing leaves the index alone"
    );
    assert_success(
        &run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&ex),
            &["--type", "analyzer", "--closeIndex"],
        ))
        .await,
    );
    assert_eq!(analyzer(&ex).await, "whitespace");
    assert_eq!(index_status(&c, &ex).await, "open");
    // injected failure while closed: the index is reopened
    let bad = tmp_path("api_an_bad", "json");
    std::fs::write(&bad, good.replace("whitespace", "no_such_tokenizer")).unwrap();
    let ex2 = format!("{p}_ex2");
    c.ok(
        "PUT",
        &ex2,
        Some(json!({"settings": {"number_of_replicas": 0}})),
    )
    .await;
    let o = run(io(
        &c,
        bad.to_str().unwrap(),
        &c.url_with_auth(&ex2),
        &["--type", "analyzer", "--closeIndex"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("reopened"), "{err}");
    assert_eq!(index_status(&c, &ex2).await, "open");
    for f in [&file, &bad] {
        let _ = std::fs::remove_file(f);
    }
    for i in [&src, &fresh, &ex, &ex2] {
        c.delete_index(i).await;
    }
}

/// Aliases of `indices` as returned by `GET <indices>/_alias`.
async fn aliases_of(c: &Cluster, indices: &[&str]) -> Value {
    c.ok("GET", &format!("{}/_alias", indices.join(",")), None)
        .await
}

// L-418 (TST-API-034): alias round trip (dump, remove, restore through one atomic
// `POST /_aliases`) keeps `is_write_index`, `is_hidden` (ES >= 7.7, OpenSearch), `filter`
// and `index_routing`/`search_routing`; per-index and host-only dumps.
#[tokio::test]
async fn alias_round_trip_keeps_every_property() {
    use serde_json::json;
    let c = Cluster::from_env();
    let v = c.vnum().await;
    let es = matches!(c.engine, Engine::Elasticsearch);
    let p = unique("api_alias");
    let (a, b) = (format!("{p}_a"), format!("{p}_b"));
    for i in [&a, &b] {
        c.ok("PUT", i, Some(json!({"settings": {"number_of_replicas": 0}, "mappings": {"properties": {"k": {"type": "keyword"}}}}))).await;
    }
    let mut actions = vec![
        json!({"add": {"index": a, "alias": format!("{p}_w"), "is_write_index": true}}),
        json!({"add": {"index": b, "alias": format!("{p}_w"), "is_write_index": false}}),
        json!({"add": {"index": a, "alias": format!("{p}_f"), "filter": {"term": {"k": "x"}}, "index_routing": "ir", "search_routing": "s1,s2"}}),
    ];
    if !es || v >= (7, 7) {
        actions.push(json!({"add": {"index": b, "alias": format!("{p}_h"), "is_hidden": true}}));
    }
    c.ok("POST", "_aliases", Some(json!({"actions": actions})))
        .await;
    let want = aliases_of(&c, &[&a, &b]).await;
    for input in [c.url_with_auth(&format!("{p}_*")), c.url_with_auth("")] {
        let file = tmp_path("api_alias", "json");
        assert_success(&run(io(&c, &input, file.to_str().unwrap(), &["--type", "alias"])).await);
        // remove every alias, then restore from the dump
        c.ok(
            "POST",
            "_aliases",
            Some(json!({"actions": [{"remove": {"index": format!("{p}_*"), "alias": "*"}}]})),
        )
        .await;
        assert!(
            aliases_of(&c, &[&a, &b]).await[&a]["aliases"]
                .as_object()
                .unwrap()
                .is_empty()
        );
        let o = run_loud(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(""),
            &["--type", "alias"],
        ))
        .await;
        assert_success(&o);
        assert_eq!(
            aliases_of(&c, &[&a, &b]).await,
            want,
            "input {}",
            input.split('@').next_back().unwrap()
        );
        let _ = std::fs::remove_file(file);
    }
    for i in [&a, &b] {
        c.delete_index(i).await;
    }
}

fn assert_warnings_once(err: &str) {
    let mut seen = std::collections::HashSet::new();
    for l in err.lines().filter(|l| l.contains("server warning:")) {
        let text = l
            .split("server warning:")
            .nth(1)
            .unwrap()
            .trim()
            .to_string();
        assert!(
            seen.insert(text.clone()),
            "warning logged twice: {text}\n{err}"
        );
    }
}

// L-419 (TST-API-035): legacy `_template` round trip on every version; ES >= 8 answers with
// deprecation `Warning` headers, which are logged once each and never fatal.
#[tokio::test]
async fn legacy_template_round_trip_with_warnings_once() {
    use serde_json::json;
    let c = Cluster::from_env();
    let name = unique("api_ltpl");
    c.ok(
        "PUT",
        &format!("_template/{name}"),
        Some(json!({"index_patterns": [format!("{name}-*")], "order": 3, "settings": {"number_of_shards": 1},
                    "mappings": {"properties": {"k": {"type": "keyword"}}}, "aliases": {format!("{name}_al"): {}}})),
    )
    .await;
    let want = c.ok("GET", &format!("_template/{name}"), None).await;
    let file = tmp_path("api_ltpl", "json");
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&name),
        file.to_str().unwrap(),
        &["--type", "template"],
    ))
    .await;
    assert_success(&o);
    assert_warnings_once(&String::from_utf8_lossy(&o.stderr));
    c.ok("DELETE", &format!("_template/{name}"), None).await;
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "template"],
    ))
    .await;
    assert_success(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_warnings_once(&err);
    if matches!(c.engine, Engine::Elasticsearch) && c.major().await >= 8 {
        println!(
            "ES {} template restore warnings: {:?}",
            c.version().await,
            err.lines()
                .filter(|l| l.contains("server warning"))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(c.ok("GET", &format!("_template/{name}"), None).await, want);
    let _ = std::fs::remove_file(file);
    c.ok("DELETE", &format!("_template/{name}"), None).await;
}

// L-420 (TST-API-036): component + composable index templates round trip (`composed_of`
// order, `priority`, `data_stream`, `_meta`), and `--filterManaged` drops exactly the
// version's `_meta.managed: true` templates (built-in list snapshotted per version in
// tests/golden/<engine>-<version>/builtins.json by the golden recorder).
#[tokio::test]
async fn composable_templates_round_trip_and_filter_managed() {
    use serde_json::json;
    let c = Cluster::from_env();
    let v = c.vnum().await;
    let es = matches!(c.engine, Engine::Elasticsearch);
    if es && v < (7, 8) {
        println!("SKIP: composable templates need ES >= 7.8");
        return;
    }
    let p = unique("api_ctpl");
    let (c1, c2, it) = (format!("{p}_c1"), format!("{p}_c2"), format!("{p}_it"));
    c.ok(
        "PUT",
        &format!("_component_template/{c1}"),
        Some(json!({"template": {"settings": {"number_of_shards": 1}}, "_meta": {"owner": "a"}})),
    )
    .await;
    c.ok("PUT", &format!("_component_template/{c2}"), Some(json!({"template": {"mappings": {"properties": {"k": {"type": "keyword"}}}}, "version": 3}))).await;
    let ds = !es || !c.is_oss().await;
    let mut body = json!({"index_patterns": [format!("{p}-*")], "composed_of": [c2, c1], "priority": 777, "_meta": {"x": 1}});
    if ds {
        body["data_stream"] = json!({});
    }
    c.ok("PUT", &format!("_index_template/{it}"), Some(body))
        .await;
    let get = |path: String| {
        let c = &c;
        async move { c.ok("GET", &path, None).await }
    };
    let want_c = get(format!("_component_template/{p}_c*")).await;
    let want_i = get(format!("_index_template/{it}")).await;
    let fc = tmp_path("api_ctpl_c", "json");
    let fi = tmp_path("api_ctpl_i", "json");
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&format!("{p}_c*")),
            fc.to_str().unwrap(),
            &["--type", "component_template"],
        ))
        .await,
    );
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&it),
            fi.to_str().unwrap(),
            &["--type", "index_template"],
        ))
        .await,
    );
    c.ok("DELETE", &format!("_index_template/{it}"), None).await;
    c.ok("DELETE", &format!("_component_template/{c1}"), None)
        .await;
    c.ok("DELETE", &format!("_component_template/{c2}"), None)
        .await;
    assert_success(
        &run(io(
            &c,
            fc.to_str().unwrap(),
            &c.url_with_auth(""),
            &["--type", "component_template"],
        ))
        .await,
    );
    assert_success(
        &run(io(
            &c,
            fi.to_str().unwrap(),
            &c.url_with_auth(""),
            &["--type", "index_template"],
        ))
        .await,
    );
    assert_eq!(get(format!("_component_template/{p}_c*")).await, want_c);
    assert_eq!(get(format!("_index_template/{it}")).await, want_i);
    assert_eq!(
        want_i["index_templates"][0]["index_template"]["composed_of"],
        json!([c2, c1])
    );

    // --filterManaged against this version's own managed list (--filterSystemTemplates off,
    // since its default name regex also drops `logs`, `metrics` and dot-prefixed templates)
    let all = get("_index_template".into()).await;
    let managed: std::collections::BTreeSet<String> = all["index_templates"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["index_template"]["_meta"]["managed"] == json!(true))
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect();
    let names = |f: &Path| -> std::collections::BTreeSet<String> {
        let l = &lines(f)[0];
        let v: Value = serde_json::from_str(l).unwrap();
        let v: Value = v
            .as_str()
            .map(|s| serde_json::from_str(s).unwrap())
            .unwrap_or(v);
        v["index_templates"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    };
    for (flag, keep_managed) in [("true", false), ("false", true)] {
        let f = tmp_path("api_ctpl_all", "json");
        assert_success(
            &run(io(
                &c,
                &c.url_with_auth(""),
                f.to_str().unwrap(),
                &[
                    "--type",
                    "index_template",
                    "--filterManaged",
                    flag,
                    "--filterSystemTemplates",
                    "false",
                ],
            ))
            .await,
        );
        let got = names(&f);
        assert!(got.contains(&it), "--filterManaged {flag}: {got:?}");
        assert_eq!(
            managed.iter().all(|m| got.contains(m)),
            keep_managed || managed.is_empty(),
            "--filterManaged {flag}: managed {managed:?} got {got:?}"
        );
        if !keep_managed {
            assert!(
                managed.iter().all(|m| !got.contains(m)),
                "{managed:?} in {got:?}"
            );
        }
        let _ = std::fs::remove_file(f);
    }
    println!("managed index templates: {managed:?}");
    for f in [&fc, &fi] {
        let _ = std::fs::remove_file(f);
    }
    c.ok("DELETE", &format!("_index_template/{it}"), None).await;
    c.ok("DELETE", &format!("_component_template/{c1}"), None)
        .await;
    c.ok("DELETE", &format!("_component_template/{c2}"), None)
        .await;
}

// L-421 (TST-API-037): ILM (ES default distribution) and ISM (OpenSearch) round trip with
// 1,050 policies (ISM pages of 1,000: paging past 20 and past 1,000); restoring over existing
// ISM policies overwrites them with if_seq_no/if_primary_term; ES OSS fails clearly.
#[tokio::test]
async fn policies_1050_round_trip_paging_and_overwrite() {
    use serde_json::json;
    let c = Cluster::from_env();
    let es = matches!(c.engine, Engine::Elasticsearch);
    let p = unique("api_pol");
    if es && c.is_oss().await {
        let f = tmp_path("api_pol_oss", "json");
        let o = run(io(
            &c,
            &c.url_with_auth(""),
            f.to_str().unwrap(),
            &["--type", "policy"],
        ))
        .await;
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains("ILM is an X-Pack feature") && err.contains("OSS"),
            "{err}"
        );
        assert!(!f.exists());
        return;
    }
    const N: usize = 1050;
    // ILM `_meta` exists from ES 7.14
    let ilm_meta = es && c.vnum().await >= (7, 14);
    let path = |i: usize| {
        if es {
            format!("_ilm/policy/{p}_{i:04}")
        } else {
            format!("_plugins/_ism/policies/{p}_{i:04}")
        }
    };
    let body = |i: usize| {
        if es {
            {
                let mut b = json!({"policy": {"phases": {"hot": {"min_age": "0ms", "actions": {"set_priority": {"priority": i % 100}}}}}});
                if ilm_meta {
                    b["policy"]["_meta"] = json!({"i": i});
                }
                b
            }
        } else {
            json!({"policy": {"description": format!("p{i}"), "default_state": "hot",
                              "states": [{"name": "hot", "actions": [], "transitions": []}]}})
        }
    };
    for i in 0..N {
        c.ok("PUT", &path(i), Some(body(i))).await;
    }
    let read = |i: usize| {
        let c = &c;
        let path = path(i);
        async move {
            let mut v = c.ok("GET", &path, None).await;
            if es {
                v.as_object_mut().unwrap().values_mut().for_each(|x| {
                    x.as_object_mut().map(|o| o.remove("modified_date"));
                    x.as_object_mut().map(|o| o.remove("version"));
                });
            } else {
                v = v["policy"].clone();
                v.as_object_mut().map(|o| o.remove("last_updated_time"));
                v.as_object_mut().map(|o| o.remove("policy_id"));
            }
            v
        }
    };
    let sample = [0usize, 19, 20, 999, 1000, N - 1];
    let mut want = vec![];
    for &i in &sample {
        want.push(read(i).await);
    }
    let file = tmp_path("api_pol", "json");
    let o = run(io(
        &c,
        &c.url_with_auth(&format!("{p}_*")),
        file.to_str().unwrap(),
        &["--type", "policy"],
    ))
    .await;
    assert_success(&o);
    let doc: Value = {
        let v: Value = serde_json::from_str(&lines(&file)[0]).unwrap();
        v.as_str()
            .map(|s| serde_json::from_str(s).unwrap())
            .unwrap_or(v)
    };
    let count = if es {
        doc.as_object().unwrap().len()
    } else {
        doc["policies"].as_array().unwrap().len()
    };
    assert_eq!(count, N, "every policy dumped");
    if !es {
        // overwrite in place: every PUT 409s and is retried with if_seq_no/if_primary_term
        let o = run_loud(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(""),
            &["--type", "policy"],
        ))
        .await;
        assert_success(&o);
        assert!(
            String::from_utf8_lossy(&o.stderr).contains("Total Writes: 1050"),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    for i in 0..N {
        c.ok("DELETE", &path(i), None).await;
    }
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "policy"],
    ))
    .await;
    assert_success(&o);
    for (k, &i) in sample.iter().enumerate() {
        assert_eq!(read(i).await, want[k], "policy {i}");
    }
    let _ = std::fs::remove_file(file);
    for i in 0..N {
        let _ = c.call("DELETE", &path(i), None).await;
    }
}

// L-651 (TST-BRK-ILM-001, extra): OpenSearch accepts every converted policy. Every ILM
// action (and an unknown one) in every phase, with no options, each option alone, and all of
// its convertible options, is restored with --convertIlmToIsm --lenientPolicyConversion:
// every policy is written (unconvertible parts dropped, on 1.3 also shrink) and reads back
// as a valid ISM policy; one run without --lenientPolicyConversion over the policies that
// need nothing dropped writes the same policies.
/// (option, value, convertible)
type L651Option = (&'static str, Value, bool);

#[tokio::test]
async fn l651_opensearch_accepts_every_converted_ilm_policy() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) {
        return;
    }
    let p = unique("api_ilm");
    let actions: &[(&str, &[L651Option])] = &[
        ("set_priority", &[("priority", json!(50), true)]),
        ("unfollow", &[]),
        (
            "rollover",
            &[
                ("max_age", json!("1d"), true),
                ("max_docs", json!(10), true),
                ("max_size", json!("5gb"), true),
                ("max_primary_shard_size", json!("1gb"), true),
                ("max_primary_shard_docs", json!(5), false),
                ("min_docs", json!(1), false),
                ("min_primary_shard_size", json!("1gb"), false),
            ],
        ),
        ("readonly", &[]),
        ("downsample", &[("fixed_interval", json!("1h"), true)]),
        (
            "shrink",
            &[
                ("number_of_shards", json!(1), true),
                ("allow_write_after_shrink", json!(true), false),
            ],
        ),
        (
            "forcemerge",
            &[
                ("max_num_segments", json!(1), true),
                ("index_codec", json!("best_compression"), false),
            ],
        ),
        (
            "searchable_snapshot",
            &[("snapshot_repository", json!("r"), true)],
        ),
        (
            "allocate",
            &[
                ("number_of_replicas", json!(0), true),
                ("require", json!({"box": "w"}), true),
                ("include", json!({"zone": "a"}), true),
                ("exclude", json!({"zone": "b"}), true),
                ("total_shards_per_node", json!(1), false),
            ],
        ),
        ("migrate", &[("enabled", json!(false), true)]),
        ("freeze", &[]),
        ("wait_for_snapshot", &[("policy", json!("s"), true)]),
        (
            "delete",
            &[("delete_searchable_snapshot", json!(true), true)],
        ),
        ("future_action", &[]),
    ];
    let mut policies: Vec<Value> = Vec::new();
    for phase in ["hot", "warm", "cold", "frozen", "delete"] {
        for (a, opts) in actions {
            let mut variants: Vec<serde_json::Map<String, Value>> = vec![Default::default()];
            for (k, v, _) in opts.iter() {
                variants.push([(k.to_string(), v.clone())].into_iter().collect());
            }
            variants.push(
                opts.iter()
                    .filter(|(_, _, ok)| *ok)
                    .map(|(k, v, _)| (k.to_string(), v.clone()))
                    .collect(),
            );
            variants.push(json!({"future_option": 1}).as_object().unwrap().clone());
            for cfg in variants {
                let mut phases = json!({"hot": {"min_age": "0ms", "actions": {"set_priority": {"priority": 1}}}});
                let def = phases
                    .as_object_mut()
                    .unwrap()
                    .entry(phase)
                    .or_insert_with(|| json!({"min_age": "3d", "actions": {}}));
                def["actions"][*a] = Value::Object(cfg);
                if phase != "delete" {
                    phases["delete"] = json!({"min_age": "30d", "actions": {"delete": {}}});
                }
                policies.push(phases);
            }
        }
    }
    // whole policies: every phase and age form at once
    policies.push(json!({
        "hot": {"min_age": "0ms", "actions": {"rollover": {"max_age": "1d", "max_primary_shard_size": "50gb"}, "set_priority": {"priority": 100}}},
        "warm": {"min_age": "90m", "actions": {"forcemerge": {"max_num_segments": 1}, "allocate": {"number_of_replicas": 0, "require": {"box": "warm"}}, "shrink": {"number_of_shards": 1}, "readonly": {}}},
        "cold": {"min_age": "7d", "actions": {"set_priority": {"priority": 0}, "searchable_snapshot": {"snapshot_repository": "r"}}},
        "frozen": {"min_age": "14d", "actions": {"searchable_snapshot": {"snapshot_repository": "r"}}},
        "delete": {"min_age": "30d", "actions": {"delete": {}}}
    }));
    policies.push(json!({"warm": {"min_age": "2d", "actions": {"readonly": {}}}, "delete": {"min_age": "0", "actions": {"delete": {}}}}));
    // L-651a: wait_for_snapshot is refused in both modes (dropping it would delete without
    // waiting for the snapshot): each such policy alone is refused, naming policy and phase,
    // and nothing is written; the rest go through as before
    let (wfs, policies): (Vec<Value>, Vec<Value>) = policies
        .into_iter()
        .partition(|ph| ph.to_string().contains("wait_for_snapshot"));
    assert!(wfs.len() >= 5, "{}", wfs.len());
    for (i, ph) in wfs.iter().enumerate() {
        let wname = format!("{p}_wfs_{i}");
        let wfile = tmp_path("api_ilm_wfs", "json");
        std::fs::write(
            &wfile,
            json!({ &wname: {"policy": {"phases": ph}} }).to_string() + "\n",
        )
        .unwrap();
        let o = run(io(
            &c,
            wfile.to_str().unwrap(),
            &c.url_with_auth(""),
            &[
                "--type",
                "policy",
                "--convertIlmToIsm",
                "--lenientPolicyConversion",
            ],
        ))
        .await;
        let e = String::from_utf8_lossy(&o.stderr);
        let phase = ph
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, d)| d["actions"].get("wait_for_snapshot").is_some())
            .map(|(k, _)| k.clone())
            .unwrap();
        assert_eq!(o.status.code(), Some(1), "{ph}: {e}");
        assert!(
            e.contains(&format!("ILM policy '{wname}'"))
                && e.contains(&format!("phase '{phase}' action 'wait_for_snapshot'"))
                && e.contains("not dropped by --lenientPolicyConversion"),
            "{ph}: {e}"
        );
        // 404, or 503 on a fresh cluster whose ISM config index does not exist yet
        let (status, body) = c
            .call("GET", &format!("_plugins/_ism/policies/{wname}"), None)
            .await;
        assert!(
            [404, 503].contains(&status),
            "{wname} written: {status} {body}"
        );
        let _ = std::fs::remove_file(wfile);
    }
    let n = policies.len();
    let name = |i: usize| format!("{p}_{i:04}");
    let doc: serde_json::Map<String, Value> = policies
        .iter()
        .enumerate()
        .map(|(i, ph)| (name(i), json!({"version": 1, "policy": {"phases": ph}})))
        .collect();
    let file = tmp_path("api_ilm", "json");
    std::fs::write(&file, Value::Object(doc).to_string() + "\n").unwrap();
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(""),
        &[
            "--type",
            "policy",
            "--convertIlmToIsm",
            "--lenientPolicyConversion",
        ],
    ))
    .await;
    assert_success(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(err.contains(&format!("Total Writes: {n}")), "{err}");
    assert!(
        err.contains("DROPPED phase 'cold' action 'searchable_snapshot'"),
        "{err}"
    );
    let mut sent = Vec::new();
    for i in 0..n {
        let v = c
            .ok("GET", &format!("_plugins/_ism/policies/{}", name(i)), None)
            .await;
        let pol = &v["policy"];
        let states = pol["states"]
            .as_array()
            .unwrap_or_else(|| panic!("{i}: {v}"));
        assert!(!states.is_empty(), "{i}: {v}");
        assert_eq!(pol["default_state"], states[0]["name"], "{i}: {v}");
        sent.push(states.clone());
    }
    // strict, over the policies the lenient run dropped nothing from: same result
    let strict: serde_json::Map<String, Value> = policies
        .iter()
        .enumerate()
        .filter(|(i, _)| !err.contains(&format!("ILM → ISM '{}': DROPPED", name(*i))))
        .map(|(i, ph)| {
            (
                format!("{}_strict", name(i)),
                json!({"policy": {"phases": ph}, "i": i}),
            )
        })
        .collect();
    assert!(strict.len() > 40, "{}", strict.len());
    let sfile = tmp_path("api_ilm_strict", "json");
    let idx: Vec<(String, usize)> = strict
        .iter()
        .map(|(k, v)| (k.clone(), v["i"].as_u64().unwrap() as usize))
        .collect();
    std::fs::write(&sfile, Value::Object(strict).to_string() + "\n").unwrap();
    let o = run_loud(io(
        &c,
        sfile.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "policy", "--convertIlmToIsm"],
    ))
    .await;
    assert_success(&o);
    for (k, i) in &idx {
        let v = c
            .ok("GET", &format!("_plugins/_ism/policies/{k}"), None)
            .await;
        assert_eq!(v["policy"]["states"].as_array().unwrap(), &sent[*i], "{k}");
        let _ = c
            .call("DELETE", &format!("_plugins/_ism/policies/{k}"), None)
            .await;
    }
    for i in 0..n {
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_ism/policies/{}", name(i)),
                None,
            )
            .await;
    }
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(sfile);
}

// L-651a (user decision): the cluster rejects a policy partway through a set (an unknown
// ISM action, injected as the third of four). OpenSearch 1.3.20, 2.19.7 and 3.9.0: the
// policy this run created is deleted, the one it overwrote gets its previous body back, the
// one after the rejection is never written, and the error says so.
#[tokio::test]
async fn l651a_a_policy_rejected_mid_set_is_rolled_back() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) {
        return;
    }
    let p = unique("rb");
    let state = |d: &str, action: Value| json!({"description": d, "default_state": "hot", "states": [{"name": "hot", "actions": [action], "transitions": []}]});
    let old = state("OLD", json!({"read_only": {}}));
    let (st, body) = c
        .call(
            "PUT",
            &format!("_plugins/_ism/policies/{p}_old"),
            Some(json!({ "policy": old })),
        )
        .await;
    assert!(st < 300, "{st} {body}");
    let names = [
        format!("{p}_new0"),
        format!("{p}_old"),
        format!("{p}_bad"),
        format!("{p}_new3"),
    ];
    let mut doc = serde_json::Map::new();
    doc.insert(
        names[0].clone(),
        json!({"policy": state("NEW", json!({"read_only": {}}))}),
    );
    doc.insert(
        names[1].clone(),
        json!({"policy": state("NEW", json!({"read_write": {}}))}),
    );
    doc.insert(
        names[2].clone(),
        json!({"policy": state("NEW", json!({"no_such_ism_action": {}}))}),
    );
    doc.insert(
        names[3].clone(),
        json!({"policy": state("NEW", json!({"read_only": {}}))}),
    );
    let file = tmp_path("api_ism_rollback", "json");
    std::fs::write(&file, Value::Object(doc).to_string() + "\n").unwrap();
    let o = run(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "policy"],
    ))
    .await;
    let e = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(
        e.contains(&format!("restoring policy '{}' (the cluster rejected it)", names[2]))
            && e.contains(&format!(
                "rolled back this run's 2 write(s): deleted the policies it had created: {}; put back the previous version of the policies it had overwritten: {}. Rollback succeeded",
                names[0], names[1]
            )),
        "{e}"
    );
    for n in [&names[0], &names[2], &names[3]] {
        let (st, body) = c
            .call("GET", &format!("_plugins/_ism/policies/{n}"), None)
            .await;
        assert_eq!(st, 404, "{n}: {body}");
    }
    let v = c
        .ok("GET", &format!("_plugins/_ism/policies/{}", names[1]), None)
        .await;
    assert_eq!(v["policy"]["description"], "OLD", "{v}");
    assert_eq!(
        v["policy"]["states"][0]["actions"][0]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| !["retry", "timeout"].contains(&k.as_str()))
            .collect::<Vec<_>>(),
        ["read_only"],
        "{v}"
    );
    let _ = c
        .call(
            "DELETE",
            &format!("_plugins/_ism/policies/{}", names[1]),
            None,
        )
        .await;
    let _ = std::fs::remove_file(file);
}

// L-652 (TST-BRK-ILM-002): odd but valid ILM policies, restored strictly with
// --convertIlmToIsm, are accepted by OpenSearch (1.3.20, 2.19.7, 3.9.0) and read back with
// the states in lifecycle order (phases given out of JSON order), forward-only transitions
// and every time unit kept as written (`ms`, `micros`, `nanos` included): missing
// `min_age`, only a delete phase, a rollover-only hot phase, nested `_meta`, a unicode name.
#[tokio::test]
async fn l652_odd_but_valid_ilm_policies_are_accepted_as_written() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) {
        return;
    }
    let p = unique("api_ilm652");
    // (name suffix, ILM phases, expected states: (name, transition condition))
    type Want = Vec<(&'static str, Option<Value>)>;
    let mut cases: Vec<(String, Value, Want)> = vec![
        (
            "order".into(),
            json!({
                "delete": {"min_age": "30d", "actions": {"delete": {}}},
                "cold": {"min_age": "14d", "actions": {"set_priority": {"priority": 0}}},
                "warm": {"min_age": "7d", "actions": {"readonly": {}}},
                "hot": {"min_age": "0ms", "actions": {"set_priority": {"priority": 100}}}
            }),
            vec![
                ("hot", Some(json!({"min_index_age": "7d"}))),
                ("warm", Some(json!({"min_index_age": "14d"}))),
                ("cold", Some(json!({"min_index_age": "30d"}))),
                ("delete", None),
            ],
        ),
        (
            "nominage".into(),
            json!({
                "warm": {"actions": {"readonly": {}}},
                "hot": {"actions": {"set_priority": {"priority": 1}}},
                "delete": {"actions": {"delete": {}}}
            }),
            vec![
                ("hot", Some(json!({}))),
                ("warm", Some(json!({}))),
                ("delete", None),
            ],
        ),
        (
            "deleteonly".into(),
            json!({"delete": {"actions": {"delete": {}}}}),
            vec![("delete", None)],
        ),
        (
            "rolloveronly".into(),
            json!({"hot": {"min_age": "0ms", "actions": {"rollover": {"max_primary_shard_size": "50gb", "max_age": "30d"}}}}),
            vec![("hot", None)],
        ),
        (
            "politique-é-日本".into(),
            json!({"hot": {"actions": {"set_priority": {"priority": 1}}}}),
            vec![("hot", None)],
        ),
    ];
    for (i, age) in [
        "0ms",
        "30s",
        "1d",
        "1000d",
        "90m",
        "12h",
        "500micros",
        "7nanos",
        "0",
        "0nanos",
    ]
    .into_iter()
    .enumerate()
    {
        let zero = age.starts_with('0');
        let cond = |k: &str| {
            if zero { json!({}) } else { json!({ k: age }) }
        };
        cases.push((
            format!("u{i}idx"),
            json!({"delete": {"min_age": age, "actions": {"delete": {}}}, "hot": {"actions": {"set_priority": {"priority": 1}}}}),
            vec![("hot", Some(cond("min_index_age"))), ("delete", None)],
        ));
        cases.push((
            format!("u{i}roll"),
            json!({"hot": {"actions": {"rollover": {"max_docs": 5}}}, "delete": {"min_age": age, "actions": {"delete": {}}}}),
            vec![("hot", Some(cond("min_rollover_age"))), ("delete", None)],
        ));
        let first: Want = if zero {
            vec![("delete", None)]
        } else {
            vec![("pending", Some(cond("min_index_age"))), ("delete", None)]
        };
        cases.push((
            format!("u{i}first"),
            json!({"delete": {"min_age": age, "actions": {"delete": {}}}}),
            first,
        ));
        if !zero {
            cases.push((
                format!("u{i}maxage"),
                json!({"hot": {"actions": {"rollover": {"max_age": age}}}}),
                vec![("hot", None)],
            ));
        }
    }
    let name = |sfx: &str| format!("{p}_{sfx}");
    let doc: serde_json::Map<String, Value> = cases
        .iter()
        .map(|(sfx, ph, _)| {
            (
                name(sfx),
                json!({"version": 1, "policy": {"phases": ph, "_meta": {"description": "odd", "owner": {"team": {"ids": [1, 2]}}}}}),
            )
        })
        .collect();
    let file = tmp_path("api_ilm652", "json");
    std::fs::write(&file, Value::Object(doc).to_string() + "\n").unwrap();
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "policy", "--convertIlmToIsm"],
    ))
    .await;
    assert_success(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(&format!("Total Writes: {}", cases.len())),
        "{err}"
    );
    assert!(!err.contains("DROPPED"), "{err}");
    for (sfx, ph, want) in &cases {
        let v = c
            .ok(
                "GET",
                &format!("_plugins/_ism/policies/{}", name(sfx)),
                None,
            )
            .await;
        let pol = &v["policy"];
        assert_eq!(pol["description"], "odd", "{sfx}: {v}");
        let states = pol["states"]
            .as_array()
            .unwrap_or_else(|| panic!("{sfx}: {v}"));
        assert_eq!(states.len(), want.len(), "{sfx}: {v}");
        assert_eq!(pol["default_state"], want[0].0, "{sfx}: {v}");
        for (i, (st, (wname, wcond))) in states.iter().zip(want).enumerate() {
            assert_eq!(st["name"], *wname, "{sfx}: {v}");
            let ts = st["transitions"].as_array().unwrap();
            match wcond {
                None => assert!(ts.is_empty(), "{sfx}: {v}"),
                Some(cond) => {
                    assert_eq!(ts.len(), 1, "{sfx}: {v}");
                    assert_eq!(ts[0]["state_name"], want[i + 1].0, "{sfx}: {v}");
                    let got = ts[0].get("conditions").cloned().unwrap_or(json!({}));
                    assert_eq!(&got, cond, "{sfx}: {v}");
                }
            }
        }
        if let Some(age) = ph.pointer("/hot/actions/rollover/max_age") {
            let r = states[0]["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|a| a.get("rollover"))
                .unwrap_or_else(|| panic!("{sfx}: {v}"));
            assert_eq!(&r["min_index_age"], age, "{sfx}: {v}");
        }
    }
    for (sfx, _, _) in &cases {
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_ism/policies/{}", name(sfx)),
                None,
            )
            .await;
    }
    let _ = std::fs::remove_file(file);
}

// L-422 (TST-API-038): on OpenSearch 1.3 a policy created through the legacy
// `_opendistro/_ism` path is visible to the tool, which only ever uses `_plugins/_ism`
// (asserted statically for the binary's endpoint inventory, and live through the recorder).
// Open Distro 1.13 (ES 7.10-based) is out of scope (Q7: not in the image matrix).
#[tokio::test]
async fn os13_legacy_opendistro_policies_are_read_via_plugins() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) || c.vnum().await >= (2, 0) {
        println!("SKIP: legacy _opendistro path check runs on OpenSearch 1.x");
        return;
    }
    let id = unique("api_odpol");
    c.ok("PUT", &format!("_opendistro/_ism/policies/{id}"), Some(json!({"policy": {"description": "legacy", "default_state": "a", "states": [{"name": "a", "actions": [], "transitions": []}]}}))).await;
    let rec = crate::support::golden::Recorder::start(&c.url_with_auth("")).await;
    let file = tmp_path("api_odpol", "json");
    let mut a = s(&[
        "--input",
        &format!("{}/{id}", rec.url()),
        "--output",
        file.to_str().unwrap(),
        "--type",
        "policy",
    ]);
    a.extend(c.tls_args("input"));
    assert_success(&run(a).await);
    let dir = tempfile::tempdir().unwrap();
    rec.save(dir.path()).unwrap();
    let mut paths = vec![];
    for e in std::fs::read_dir(dir.path()).unwrap().flatten() {
        if e.file_name().to_string_lossy().ends_with(".req.json") {
            let r: Value =
                serde_json::from_str(&std::fs::read_to_string(e.path()).unwrap()).unwrap();
            paths.push(r["path"].as_str().unwrap().to_string());
        }
    }
    assert!(
        paths
            .iter()
            .any(|p| p.starts_with("/_plugins/_ism/policies/")),
        "{paths:?}"
    );
    assert!(
        paths.iter().all(|p| !p.contains("_opendistro")),
        "{paths:?}"
    );
    assert!(std::fs::read_to_string(&file).unwrap().contains("legacy"));
    let _ = std::fs::remove_file(file);
    c.ok("DELETE", &format!("_plugins/_ism/policies/{id}"), None)
        .await;
}

// L-423 (TST-API-039): stored scripts (painless and mustache) round trip; an empty
// `stored_scripts` restores nothing and succeeds; on secured profiles a user without the
// `monitor` cluster privilege gets a 403 whose message names `monitor`.
#[tokio::test]
async fn scripts_round_trip_empty_and_monitor_privilege() {
    use serde_json::json;
    let c = Cluster::from_env();
    let p = unique("api_scr");
    let (pl, mu) = (format!("{p}_painless"), format!("{p}_mustache"));
    c.ok(
        "PUT",
        &format!("_scripts/{pl}"),
        Some(json!({"script": {"lang": "painless", "source": "doc['n'].value * params.f"}})),
    )
    .await;
    c.ok(
        "PUT",
        &format!("_scripts/{mu}"),
        Some(
            json!({"script": {"lang": "mustache", "source": {"query": {"term": {"k": "{{k}}"}}}}}),
        ),
    )
    .await;
    let want_pl = c.ok("GET", &format!("_scripts/{pl}"), None).await;
    let want_mu = c.ok("GET", &format!("_scripts/{mu}"), None).await;
    let file = tmp_path("api_scr", "json");
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&format!("{p}_*")),
            file.to_str().unwrap(),
            &["--type", "script"],
        ))
        .await,
    );
    for id in [&pl, &mu] {
        c.ok("DELETE", &format!("_scripts/{id}"), None).await;
    }
    assert_success(
        &run(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(""),
            &["--type", "script"],
        ))
        .await,
    );
    assert_eq!(c.ok("GET", &format!("_scripts/{pl}"), None).await, want_pl);
    assert_eq!(c.ok("GET", &format!("_scripts/{mu}"), None).await, want_mu);
    // empty stored_scripts
    let empty = tmp_path("api_scr_empty", "json");
    std::fs::write(&empty, "{\"stored_scripts\":[]}\n").unwrap();
    let o = run_loud(io(
        &c,
        empty.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "script"],
    ))
    .await;
    assert_success(&o);
    for f in [&file, &empty] {
        let _ = std::fs::remove_file(f);
    }
    for id in [&pl, &mu] {
        c.ok("DELETE", &format!("_scripts/{id}"), None).await;
    }

    if !is_secure() {
        println!("SKIP: monitor-privilege check needs a secured profile");
        return;
    }
    let user = format!("{p}_low");
    let pass = "Lowpriv-Pass-12345!x";
    let role = format!("{p}_role");
    let es = matches!(c.engine, Engine::Elasticsearch);
    if es {
        c.ok(
            "PUT",
            &format!("_security/role/{role}"),
            Some(json!({"indices": [{"names": ["*"], "privileges": ["read"]}]})),
        )
        .await;
        c.ok(
            "PUT",
            &format!("_security/user/{user}"),
            Some(json!({"password": pass, "roles": [role]})),
        )
        .await;
    } else {
        os_security_put(
            &c,
            &format!("_plugins/_security/api/roles/{role}"),
            json!({"index_permissions": [{"index_patterns": ["*"], "allowed_actions": ["read"]}]}),
        )
        .await;
        os_security_put(
            &c,
            &format!("_plugins/_security/api/internalusers/{user}"),
            json!({"password": pass}),
        )
        .await;
        os_security_put(
            &c,
            &format!("_plugins/_security/api/rolesmapping/{role}"),
            json!({"users": [user]}),
        )
        .await;
    }
    let mut u = url::Url::parse(&c.index_url("")).unwrap();
    u.set_username(&user).unwrap();
    u.set_password(Some(pass)).unwrap();
    let f = tmp_path("api_scr_low", "json");
    let mut a = s(&[
        "--input",
        u.as_str(),
        "--output",
        f.to_str().unwrap(),
        "--type",
        "script",
    ]);
    a.extend(c.tls_args("input"));
    let o = run(a).await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("403") && err.contains("'monitor' cluster privilege"),
        "{err}"
    );
    assert!(!err.contains(pass), "password leaked: {err}");
    assert!(!f.exists());
    if es {
        let _ = c
            .call("DELETE", &format!("_security/user/{user}"), None)
            .await;
        let _ = c
            .call("DELETE", &format!("_security/role/{role}"), None)
            .await;
    } else {
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/rolesmapping/{role}"),
                None,
            )
            .await;
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/internalusers/{user}"),
                None,
            )
            .await;
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/roles/{role}"),
                None,
            )
            .await;
    }
}

// L-429 (TST-API-053): version conflicts. `create` on existing ids and `--handleVersion
// --versionType external` with a lower version are counted per item like the server reports
// them; the exit code follows --ignore-es-write-errors (default: continue, exit 3 + Failed
// Writes since L-444; explicit true: exit 0; false: exit 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn version_conflicts_counted_and_exit_follows_ignore_flags() {
    use serde_json::json;
    let c = Cluster::from_env();
    let dst = unique("api_vc");
    c.delete_index(&dst).await;
    c.ok(
        "PUT",
        &dst,
        Some(json!({"settings": {"number_of_shards": 1, "number_of_replicas": 0}})),
    )
    .await;
    // existing: e0..e4 at external version 5
    for i in 0..5 {
        c.ok(
            "PUT",
            &format!("{dst}/_doc/e{i}?version=5&version_type=external"),
            Some(json!({"n": i, "orig": true})),
        )
        .await;
    }
    c.ok("POST", &format!("{dst}/_refresh"), None).await;
    // create: e0..e4 conflict, n0..n4 are new
    let mut input = String::new();
    for i in 0..5 {
        input.push_str(&format!(
            "{{\"_index\":\"x\",\"_id\":\"e{i}\",\"_source\":{{\"n\":{i}}}}}\n"
        ));
        input.push_str(&format!(
            "{{\"_index\":\"x\",\"_id\":\"n{i}\",\"_source\":{{\"n\":{i}}}}}\n"
        ));
    }
    let file = tmp_path("api_vc", "json");
    std::fs::write(&file, &input).unwrap();
    let dst_url = c.url_with_auth(&dst);
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &dst_url,
        &["--bulkAction", "create"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert_eq!(report_counts(&o), (5, 5), "{err}");
    assert!(err.contains("version_conflict_engine_exception"), "{err}");
    // the same with --ignore-es-write-errors false: exit 1, counts unchanged semantics
    c.delete_index(&dst).await;
    c.ok(
        "PUT",
        &dst,
        Some(json!({"settings": {"number_of_shards": 1, "number_of_replicas": 0}})),
    )
    .await;
    for i in 0..5 {
        c.ok(
            "PUT",
            &format!("{dst}/_doc/e{i}?version=5&version_type=external"),
            Some(json!({"n": i, "orig": true})),
        )
        .await;
    }
    c.ok("POST", &format!("{dst}/_refresh"), None).await;
    let o = run_loud(io(
        &c,
        file.to_str().unwrap(),
        &dst_url,
        &[
            "--bulkAction",
            "create",
            "--ignore-es-write-errors",
            "false",
        ],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("version_conflict_engine_exception"), "{err}");

    // --handleVersion --versionType external: lower (3) conflicts, higher (7) wins
    let mut vin = String::new();
    for i in 0..5 {
        let v = if i % 2 == 0 { 3 } else { 7 };
        vin.push_str(&format!(
            "{{\"_index\":\"x\",\"_id\":\"e{i}\",\"_version\":{v},\"_source\":{{\"n\":{},\"orig\":false}}}}\n",
            100 + i
        ));
    }
    let vfile = tmp_path("api_vc_ver", "json");
    std::fs::write(&vfile, &vin).unwrap();
    let o = run_loud(io(
        &c,
        vfile.to_str().unwrap(),
        &dst_url,
        &["--handleVersion", "--versionType", "external"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    // the skipped older versions are failed writes too: exit 3 unless accepted explicitly
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert_eq!(report_counts(&o), (2, 3), "{err}");
    c.ok("POST", &format!("{dst}/_refresh"), None).await;
    for i in 0..5 {
        let d = c.ok("GET", &format!("{dst}/_doc/e{i}"), None).await;
        let (want_v, want_orig) = if i % 2 == 0 { (5, true) } else { (7, false) };
        assert_eq!(d["_version"], json!(want_v), "e{i}: {d}");
        assert_eq!(d["_source"]["orig"], json!(want_orig), "e{i}: {d}");
    }
    // external_gte with an equal version is accepted
    let o = run_loud(io(
        &c,
        vfile.to_str().unwrap(),
        &dst_url,
        &["--handleVersion", "--versionType", "external_gte"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(
        report_counts(&o),
        (2, 3),
        "external_gte: 7>=7 ok, 3<5 conflicts: {err}"
    );
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(vfile);
    c.delete_index(&dst).await;
}

// L-430 (TST-API-054): auth failures. No credentials, a wrong password, a disabled user (ES)
// and missing privileges per endpoint each give exit 1 and one message naming the endpoint and
// the privilege; separate input/output credentials are kept apart; no password is printed.
#[tokio::test]
async fn auth_failures_name_endpoint_and_privilege() {
    use serde_json::json;
    if !is_secure() {
        println!("SKIP: auth failures need a secured profile");
        return;
    }
    let c = Cluster::from_env();
    let es = matches!(c.engine, Engine::Elasticsearch);
    let p = unique("api_auth");
    let src = format!("{p}_src");
    let tpl = format!("{p}_tpl");
    c.delete_index(&src).await;
    c.ok(
        "PUT",
        &format!("{src}/_doc/1?refresh=true"),
        Some(json!({"n": 1})),
    )
    .await;
    c.ok(
        "PUT",
        &format!("_index_template/{tpl}"),
        Some(json!({"index_patterns": [format!("{tpl}-*")], "template": {"settings": {"number_of_replicas": 0}}})),
    )
    .await;
    let user = format!("{p}_low");
    let pass = "Lowpriv-Pass-98765!q";
    let role = format!("{p}_role");
    // low user: cluster monitor + index read only
    if es {
        c.ok(
            "PUT",
            &format!("_security/role/{role}"),
            Some(json!({"cluster": ["monitor"], "indices": [{"names": ["*"], "privileges": ["read", "view_index_metadata"]}]})),
        )
        .await;
        c.ok(
            "PUT",
            &format!("_security/user/{user}"),
            Some(json!({"password": pass, "roles": [role]})),
        )
        .await;
    } else {
        // the security config is shared with tests creating users in parallel: retry on 409
        for (path, body) in [
            (
                format!("_plugins/_security/api/roles/{role}"),
                json!({"cluster_permissions": ["cluster_monitor", "cluster_composite_ops_ro"], "index_permissions": [{"index_patterns": ["*"], "allowed_actions": ["read", "indices_monitor"]}]}),
            ),
            (
                format!("_plugins/_security/api/internalusers/{user}"),
                json!({"password": pass}),
            ),
            (
                format!("_plugins/_security/api/rolesmapping/{role}"),
                json!({"users": [user]}),
            ),
        ] {
            os_security_put(&c, &path, body).await;
        }
    }
    let with = |path: &str, u: Option<(&str, &str)>| {
        let mut url = url::Url::parse(&c.index_url(path)).unwrap();
        if let Some((n, pw)) = u {
            url.set_username(n).unwrap();
            url.set_password(Some(pw)).unwrap();
        }
        url.to_string()
    };
    let check = |name: &str, o: &std::process::Output, want: &[&str]| {
        let err = String::from_utf8_lossy(&o.stderr);
        println!("--- {name}:\n{err}");
        assert_eq!(o.status.code(), Some(1), "{name}: {err}");
        for w in want {
            assert!(err.contains(w), "{name}: missing {w:?}: {err}");
        }
        let admin_pw = std::env::var("IT_ADMIN_PW").unwrap_or_default();
        // a short admin password like "admin" would match "indices:admin/create"
        let admin_pw = if admin_pw.len() >= 8 {
            admin_pw
        } else {
            "\u{0}".into()
        };
        for secret in [pass, "Wrong-Pass-55555!z", admin_pw.as_str()] {
            assert!(!err.contains(secret), "{name}: secret leaked: {err}");
            assert!(
                !String::from_utf8_lossy(&o.stdout).contains(secret),
                "{name}: secret on stdout"
            );
        }
    };
    let out = tmp_path("api_auth", "json");
    // 1. no credentials
    let o = run_loud(io(&c, &with(&src, None), out.to_str().unwrap(), &[])).await;
    check(
        "no credentials",
        &o,
        &[
            "HTTP 401",
            "authentication failed at https://",
            &format!("/{src}/_search"),
        ],
    );
    // 2. wrong password
    let o = run_loud(io(
        &c,
        &with(&src, Some((&user, "Wrong-Pass-55555!z"))),
        out.to_str().unwrap(),
        &[],
    ))
    .await;
    check(
        "wrong password",
        &o,
        &["HTTP 401", "authentication failed at"],
    );
    // 3. disabled user (Elasticsearch only; OpenSearch internal users have no disable API)
    if es {
        c.ok("PUT", &format!("_security/user/{user}/_disable"), None)
            .await;
        let o = run_loud(io(
            &c,
            &with(&src, Some((&user, pass))),
            out.to_str().unwrap(),
            &[],
        ))
        .await;
        check("disabled user", &o, &["HTTP 401", "expired or locked"]);
        c.ok("PUT", &format!("_security/user/{user}/_enable"), None)
            .await;
    }
    // 4. read works for the low user
    let o = run_loud(io(
        &c,
        &with(&src, Some((&user, pass))),
        out.to_str().unwrap(),
        &[],
    ))
    .await;
    assert_success(&o);
    // 5. separate credentials: admin input, low-privilege output -> write privilege on the
    // output. Elasticsearch rejects the whole _bulk (403); OpenSearch accepts it and fails each
    // item with a 403 security_exception (exit 3 by default since L-444; exit 1 with
    // --ignore-es-write-errors false, used here).
    let dst = format!("{p}_dst");
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&src),
        &with(&dst, Some((&user, pass))),
        &["--ignore-es-write-errors", "false"],
    ))
    .await;
    if es {
        check(
            "no write privilege",
            &o,
            &[
                "HTTP 403",
                "permission denied at https://",
                "/_bulk",
                "'write'",
            ],
        );
    } else {
        check(
            "no write privilege",
            &o,
            &["403", "security_exception", "no permissions for [indices:"],
        );
    }
    // 6. template restore without manage_index_templates
    let tfile = tmp_path("api_auth_tpl", "json");
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&tpl),
            tfile.to_str().unwrap(),
            &["--type", "index_template"],
        ))
        .await,
    );
    c.ok("DELETE", &format!("_index_template/{tpl}"), None)
        .await;
    let o = run_loud(io(
        &c,
        tfile.to_str().unwrap(),
        &with("", Some((&user, pass))),
        &["--type", "index_template"],
    ))
    .await;
    check(
        "no manage_index_templates",
        &o,
        &[
            "HTTP 403",
            &format!("/_index_template/{tpl}"),
            "'manage_index_templates'",
        ],
    );
    // 7. policy restore without manage_ilm / ISM permissions
    let pol = format!("{p}_pol");
    let pfile = tmp_path("api_auth_pol", "json");
    let (ppath, want) = if es {
        c.ok(
            "PUT",
            &format!("_ilm/policy/{pol}"),
            Some(json!({"policy": {"phases": {"hot": {"actions": {}}}}})),
        )
        .await;
        (format!("_ilm/policy/{pol}"), "'manage_ilm'")
    } else {
        c.ok("PUT", &format!("_plugins/_ism/policies/{pol}"), Some(json!({"policy": {"description": "x", "default_state": "hot", "states": [{"name": "hot", "actions": [], "transitions": []}]}}))).await;
        (format!("_plugins/_ism/policies/{pol}"), "ISM permissions")
    };
    assert_success(
        &run(io(
            &c,
            &c.url_with_auth(&pol),
            pfile.to_str().unwrap(),
            &["--type", "policy"],
        ))
        .await,
    );
    c.ok("DELETE", &ppath, None).await;
    let o = run_loud(io(
        &c,
        pfile.to_str().unwrap(),
        &with("", Some((&user, pass))),
        &["--type", "policy"],
    ))
    .await;
    check("no policy privilege", &o, &["HTTP 403", want]);

    for f in [&out, &tfile, &pfile] {
        let _ = std::fs::remove_file(f);
    }
    c.delete_index(&src).await;
    c.delete_index(&dst).await;
    if es {
        let _ = c
            .call("DELETE", &format!("_security/user/{user}"), None)
            .await;
        let _ = c
            .call("DELETE", &format!("_security/role/{role}"), None)
            .await;
    } else {
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/rolesmapping/{role}"),
                None,
            )
            .await;
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/internalusers/{user}"),
                None,
            )
            .await;
        let _ = c
            .call(
                "DELETE",
                &format!("_plugins/_security/api/roles/{role}"),
                None,
            )
            .await;
    }
}

/// PUT to the OpenSearch security API, retrying the 409 that a parallel test writing the same
/// security config index causes.
pub(crate) async fn os_security_put(c: &Cluster, path: &str, body: Value) {
    let mut tries = 0;
    loop {
        let (st, v) = c.call("PUT", path, Some(body.clone())).await;
        if st < 300 {
            return;
        }
        tries += 1;
        assert!(st == 409 && tries < 20, "PUT {path} -> {st}: {v}");
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

// L-432 (TST-API-056): server errors pass through with the server's `type` and `reason`
// (index_not_found, mapping conflict = illegal_argument, mapper_parsing on a bulk item), and
// no quoted body exceeds 2 KB.
#[tokio::test]
async fn server_error_passthrough_type_and_reason() {
    use serde_json::json;
    let c = Cluster::from_env();
    let p = unique("api_err");
    let missing = format!("{p}_missing");
    let out = tmp_path("api_err", "json");
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&missing),
        out.to_str().unwrap(),
        &[],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("HTTP 404: index_not_found_exception: no such index"),
        "{err}"
    );
    // mapping conflict: n is long in the target, text in the file
    let dst = format!("{p}_dst");
    c.delete_index(&dst).await;
    c.ok(
        "PUT",
        &dst,
        Some(json!({"mappings": {"properties": {"n": {"type": "long"}}}})),
    )
    .await;
    let mfile = tmp_path("api_err_map", "json");
    std::fs::write(
        &mfile,
        format!(
            "{{\"{dst}\":{{\"mappings\":{{\"properties\":{{\"n\":{{\"type\":\"text\"}}}}}}}}}}\n"
        ),
    )
    .unwrap();
    let o = run_loud(io(
        &c,
        mfile.to_str().unwrap(),
        &c.url_with_auth(&dst),
        &["--type", "mapping"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("HTTP 400: illegal_argument_exception: mapper [n] cannot be changed from type [long] to [text]"), "{err}");
    // mapper_parsing on a bulk item: the item's type and reason are printed
    let dfile = tmp_path("api_err_data", "json");
    std::fs::write(
        &dfile,
        "{\"_index\":\"x\",\"_id\":\"1\",\"_source\":{\"n\":\"abc\"}}\n",
    )
    .unwrap();
    let o = run_loud(io(&c, dfile.to_str().unwrap(), &c.url_with_auth(&dst), &[])).await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        (err.contains("mapper_parsing_exception") || err.contains("document_parsing_exception"))
            && err.contains("failed to parse field [n] of type [long]"),
        "{err}"
    );
    for line in err.lines() {
        assert!(line.len() <= 4096, "overlong line ({} bytes)", line.len());
    }
    for f in [&out, &mfile, &dfile] {
        let _ = std::fs::remove_file(f);
    }
    c.delete_index(&dst).await;
}

// L-653 (TST-BRK-ILM-003): Elasticsearch's own built-in policies, as `GET _ilm/policy`
// returned them on 7.17.29, 8.19.23 and 9.1.2 (golden recordings), restored into
// OpenSearch with --convertIlmToIsm. By default (--filterManaged) the ones marked
// `_meta.managed: true` are skipped and named, and the rest written; with
// --filterManaged false every one converts strictly (nothing dropped), ids with a leading
// '.' or an '@' included, OpenSearch accepts each and reads it back, and each managed one
// is reported as an Elasticsearch-managed policy that is now an ordinary ISM policy.
#[tokio::test]
async fn l653_es_builtin_policies_restore_into_opensearch() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) {
        return;
    }
    for v in ["7.17.29", "8.19.23", "9.1.2"] {
        let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "tests/golden/elasticsearch-{v}/policy/002.resp.json"
        ));
        let resp: Value = serde_json::from_slice(&std::fs::read(&golden).unwrap()).unwrap();
        let mut body: Value = match &resp["body"] {
            Value::String(s) => serde_json::from_str(s).unwrap(),
            b => b.clone(),
        };
        body.as_object_mut().unwrap().remove("golden_pol");
        let all: Vec<String> = body.as_object().unwrap().keys().cloned().collect();
        let managed: Vec<&String> = all
            .iter()
            .filter(|n| body[n.as_str()].pointer("/policy/_meta/managed") == Some(&json!(true)))
            .collect();
        assert!(!managed.is_empty(), "{v}");
        assert!(
            v == "7.17.29"
                || all.iter().any(|n| n.starts_with('.')) && all.iter().any(|n| n.contains('@')),
            "{v}: {all:?}"
        );
        let file = tmp_path("api_ilm653", "json");
        std::fs::write(&file, body.to_string() + "\n").unwrap();
        let restore = |extra: &'static [&'static str]| {
            let mut a = vec!["--type", "policy", "--convertIlmToIsm"];
            a.extend_from_slice(extra);
            run_loud(io(&c, file.to_str().unwrap(), &c.url_with_auth(""), &a))
        };
        let present = |n: String| {
            let c = &c;
            async move {
                c.call("GET", &format!("_plugins/_ism/policies/{n}"), None)
                    .await
            }
        };
        // default --filterManaged: the managed ones are skipped, and named (an info line,
        // shown with --debug)
        let o = restore(&["--debug"]).await;
        assert_success(&o);
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        let head = format!(
            "Skipping {} managed policies (--filterManaged; --filterManaged=false keeps them): ",
            managed.len()
        );
        let line = err
            .lines()
            .find_map(|l| l.split_once(head.as_str()).map(|(_, r)| r))
            .unwrap_or_else(|| panic!("{v}: {err}"));
        let mut skipped: Vec<&str> = line.split(", ").collect();
        skipped.sort_unstable();
        let mut want: Vec<&str> = managed.iter().map(|n| n.as_str()).collect();
        want.sort_unstable();
        assert_eq!(skipped, want, "{v}");
        assert!(
            err.contains(&format!("Total Writes: {}", all.len() - managed.len())),
            "{v}: {err}"
        );
        for n in &all {
            let (st, got) = present(n.clone()).await;
            assert_eq!(st == 200, !managed.contains(&n), "{v} {n}: {st} {got}");
        }
        // --filterManaged false, strict: every built-in written as converted. OpenSearch 1.x
        // has no ISM shrink: the built-ins with a warm-phase shrink are refused before any
        // write, naming policy, phase and action; --lenientPolicyConversion drops just that.
        let shrinking: Vec<&String> = all
            .iter()
            .filter(|n| {
                body[n.as_str()]
                    .pointer("/policy/phases/warm/actions/shrink")
                    .is_some()
            })
            .collect();
        let o = if c.major().await < 2 && !shrinking.is_empty() {
            let o = run(io(
                &c,
                file.to_str().unwrap(),
                &c.url_with_auth(""),
                &[
                    "--type",
                    "policy",
                    "--convertIlmToIsm",
                    "--filterManaged",
                    "false",
                ],
            ))
            .await;
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            assert_eq!(o.status.code(), Some(1), "{v}: {err}");
            assert!(
                err.contains(&format!(
                    "ILM policy '{}' cannot be converted to ISM safely: phase 'warm' action 'shrink': the target OpenSearch has no ISM shrink action",
                    shrinking[0]
                )) && err.contains("--lenientPolicyConversion drops these parts instead"),
                "{v}: {err}"
            );
            for n in &all {
                let (st, got) = present(n.clone()).await;
                assert_eq!(st == 200, !managed.contains(&n), "{v} {n}: {st} {got}");
            }
            let o = restore(&["--filterManaged", "false", "--lenientPolicyConversion"]).await;
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            let mut dropped: Vec<&str> = Vec::new();
            for (n, what) in err
                .lines()
                .filter_map(|l| l.split_once("ILM → ISM '"))
                .filter_map(|(_, r)| r.split_once("': DROPPED "))
            {
                assert!(
                    what.starts_with("phase 'warm' action 'shrink'"),
                    "{v}: {what}"
                );
                dropped.push(n);
            }
            dropped.sort_unstable();
            let mut want: Vec<&str> = shrinking.iter().map(|n| n.as_str()).collect();
            want.sort_unstable();
            assert_eq!(dropped, want, "{v}");
            o
        } else {
            let o = restore(&["--filterManaged", "false"]).await;
            let err = String::from_utf8_lossy(&o.stderr).into_owned();
            assert!(!err.contains("DROPPED"), "{v}: {err}");
            o
        };
        assert_success(&o);
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert!(
            err.contains(&format!("Total Writes: {}", all.len())),
            "{v}: {err}"
        );
        assert_eq!(
            err.matches(
                "Elasticsearch-managed (built-in) policy: on OpenSearch it is an ordinary policy"
            )
            .count(),
            managed.len(),
            "{v}: {err}"
        );
        for n in &all {
            let (st, got) = present(n.clone()).await;
            assert_eq!(st, 200, "{v} {n}: {got}");
            let pol = &got["policy"];
            assert_eq!(pol["policy_id"], json!(n), "{v} {n}: {got}");
            let states = pol["states"]
                .as_array()
                .unwrap_or_else(|| panic!("{v} {n}: {got}"));
            assert!(!states.is_empty(), "{v} {n}: {got}");
            assert_eq!(pol["default_state"], states[0]["name"], "{v} {n}: {got}");
            if body[n.as_str()]
                .pointer("/policy/phases/delete/actions/delete")
                .is_some()
            {
                let last = states.last().unwrap();
                assert_eq!(last["name"], "delete", "{v} {n}: {got}");
                assert_eq!(
                    last["actions"].as_array().unwrap().len(),
                    1,
                    "{v} {n}: {got}"
                );
                assert!(last["actions"][0].get("delete").is_some(), "{v} {n}: {got}");
            }
        }
        for n in &all {
            let _ = c
                .call("DELETE", &format!("_plugins/_ism/policies/{n}"), None)
                .await;
        }
        let _ = std::fs::remove_file(file);
    }
}

// L-654 (BRK-XV-001): OpenSearch with `compatibility.override_main_response_version: true`
// (a dynamic setting on 1.x and 2.x) answers `GET /` as Elasticsearch 7.10.2 with no distribution;
// the tool still detects OpenSearch at its real version (from `GET _nodes`) and a
// `--searchType pit` dump uses the OpenSearch PIT (2.x) or scroll (1.x), never the
// Elasticsearch one. OpenSearch 3.x rejects the setting.
#[tokio::test]
async fn l654_override_main_response_version_is_still_opensearch() {
    use serde_json::json;
    let c = Cluster::from_env();
    if !matches!(c.engine, Engine::OpenSearch) {
        return;
    }
    let real = c.version().await;
    let set = |v: Value| {
        let c = &c;
        async move {
            c.call(
                "PUT",
                "_cluster/settings",
                Some(json!({"persistent": {"compatibility.override_main_response_version": v}})),
            )
            .await
        }
    };
    let major = c.major().await;
    let (status, body) = set(json!(true)).await;
    if status != 200 {
        // removed in OpenSearch 3.0
        assert!(
            major >= 3
                && status == 400
                && body.to_string().contains("override_main_response_version"),
            "OpenSearch {real}: {status} {body}"
        );
        return;
    }
    let index = unique("api_l654");
    seed_index(&c, &index, 5).await;
    let out = tmp_path("api_l654", "json");
    let root = c.ok("GET", "", None).await;
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&index),
        out.to_str().unwrap(),
        &["--type", "data", "--searchType", "pit", "--debug"],
    ))
    .await;
    // Reset before asserting, so a failure leaves a normal cluster behind.
    let (reset, _) = set(Value::Null).await;
    let docs = c.count(&index).await;
    c.delete_index(&index).await;
    assert_eq!(reset, 200);
    assert_eq!(root["version"]["number"], "7.10.2", "{root}");
    assert!(root["version"].get("distribution").is_none(), "{root}");
    assert_success(&o);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(&format!("Detected OpenSearch {real}"))
            && err.contains(&format!("using {real} from GET _nodes")),
        "{err}"
    );
    assert!(
        !err.contains("/_pit") && (major >= 2) == err.contains("point_in_time"),
        "{err}"
    );
    assert_eq!(lines(&out).len() as u64, docs);
    assert_eq!(c.version().await, real);
}

// L-655 (BRK-XV-004): field types without an equivalent on the other engine (or this
// version). For each type the tool knows, restoring a one-field mapping file is refused
// before any write exactly when this cluster really has no such type (a direct PUT of the
// same mapping fails with "No handler for type" / "mapper type [..] does not exist" or,
// for an Elasticsearch `sparse_vector` on OpenSearch 3.x, "requires [method]"; on
// Elasticsearch 8.0-8.10, "no longer supported"), so the
// table never refuses what the cluster takes. A typical other-engine mapping is refused
// naming each field, and the index is not created.
#[tokio::test]
async fn l655_fields_without_an_equivalent_are_refused_exactly_when_missing() {
    use serde_json::json;
    let c = Cluster::from_env();
    let es = matches!(c.engine, Engine::Elasticsearch);
    let version = c.version().await;
    let defs = [
        json!({"type": "flattened"}),
        json!({"type": "flat_object"}),
        json!({"type": "dense_vector", "dims": 3}),
        json!({"type": "knn_vector", "dimension": 3}),
        json!({"type": "semantic_text"}),
        json!({"type": "semantic", "model_id": "m"}),
        json!({"type": "match_only_text"}),
        json!({"type": "wildcard"}),
        json!({"type": "constant_keyword", "value": "x"}),
        json!({"type": "unsigned_long"}),
        json!({"type": "version"}),
        json!({"type": "aggregate_metric_double", "metrics": ["min", "max"], "default_metric": "max"}),
        json!({"type": "histogram"}),
        json!({"type": "counted_keyword"}),
        json!({"type": "rank_vectors", "dims": 3}),
        json!({"type": "shape"}),
        json!({"type": "point"}),
        json!({"type": "xy_shape"}),
        json!({"type": "xy_point"}),
        json!({"type": "passthrough", "priority": 1}),
        json!({"type": "sparse_vector"}),
    ];
    let missing_on_server = |body: &Value| {
        let s = body.to_string();
        s.contains("No handler for type")
            || s.contains("does not exist. It might have been created")
            || s.contains("requires [method]")
            || s.contains("no handler for type")
            || s.contains("field type is no longer supported")
    };
    let mut refused = vec![];
    for def in &defs {
        let ty = def["type"].as_str().unwrap();
        let mapping = json!({"properties": {"k": {"type": "keyword"}, "f": def}});
        let direct = unique("api_l655_direct");
        let (st, body) = c
            .call(
                "PUT",
                &direct,
                Some(json!({"settings": {"number_of_replicas": 0}, "mappings": mapping})),
            )
            .await;
        c.delete_index(&direct).await;
        let dst = unique("api_l655");
        let file = tmp_path("api_l655", "json");
        std::fs::write(
            &file,
            json!({ &dst: {"mappings": mapping} }).to_string() + "\n",
        )
        .unwrap();
        let o = run_loud(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&dst),
            &["--type", "mapping"],
        ))
        .await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        let tool_refused = err.contains(&format!("`f` ({ty}:"));
        let (exists, _) = c.call("HEAD", &dst, None).await;
        c.delete_index(&dst).await;
        if tool_refused {
            assert_eq!(o.status.code(), Some(1), "{ty}: {err}");
            assert!(err.contains("nothing was written"), "{ty}: {err}");
            assert_eq!(exists, 404, "{ty}: refused, but the index was created");
            assert!(
                st != 200 && missing_on_server(&body),
                "{version}: the tool refuses {ty}, but the cluster has it: {st} {body}"
            );
            refused.push(ty);
        } else {
            assert!(
                st == 200 || !missing_on_server(&body),
                "{version}: the cluster has no {ty} ({body}) but the tool did not refuse it: {err}"
            );
            if st == 200 {
                assert_success(&o);
            }
        }
    }
    println!("L-655 {version}: refused {refused:?}");
    // a typical mapping from the other engine
    let other = if es {
        json!({"properties": {"msg": {"type": "text"}, "attrs": {"type": "flat_object"}, "emb": {"type": "knn_vector", "dimension": 3}}})
    } else {
        json!({"properties": {"msg": {"type": "text"}, "labels": {"type": "flattened"}, "obj": {"properties": {"emb": {"type": "dense_vector", "dims": 3}}}}})
    };
    let dst = unique("api_l655_other");
    let file = tmp_path("api_l655_other", "json");
    std::fs::write(
        &file,
        json!({ &dst: {"mappings": other} }).to_string() + "\n",
    )
    .unwrap();
    for ty in ["mapping", "index"] {
        let o = run_loud(io(
            &c,
            file.to_str().unwrap(),
            &c.url_with_auth(&dst),
            &["--type", ty],
        ))
        .await;
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(o.status.code(), Some(1), "{ty}: {err}");
        let named = if es {
            ["`attrs` (flat_object", "`emb` (knn_vector"]
        } else {
            ["`labels` (flattened", "`obj.emb` (dense_vector"]
        };
        assert!(
            named.iter().all(|n| err.contains(n)) && err.contains("2 field type(s)"),
            "{ty}: {err}"
        );
        assert_eq!(c.call("HEAD", &dst, None).await.0, 404, "{ty}");
    }
}

// L-656 (BRK-XV-005): Elasticsearch 7.10 OSS (no X-Pack) and OpenSearch 1.3 (and the other
// pinned engines) as targets. A template or index restore is refused before any write exactly
// when this cluster rejects the feature (a direct PUT answers "unknown setting" / "unknown
// field [data_stream]"), and nothing is created; ILM policies, PIT, credentials and data
// streams fail or degrade with a clear message.
#[tokio::test]
async fn l656_missing_features_on_the_target_fail_clearly() {
    use serde_json::json;
    let c = Cluster::from_env();
    let es = matches!(c.engine, Engine::Elasticsearch);
    let version = c.version().await;
    let oss = c.is_oss().await;
    let (maj, min) = c.vnum().await;
    let missing = |body: &Value| {
        let s = body.to_string();
        s.contains("unknown setting [") || s.contains("unknown field [data_stream]")
    };
    // (case, --type, template?, body)
    let cases = [
        (
            "data_stream",
            "index_template",
            true,
            json!({"data_stream": {}, "template": {"settings": {"number_of_replicas": 0}}}),
        ),
        (
            "ilm_template",
            "index_template",
            true,
            json!({"template": {"settings": {"index.lifecycle.name": "p1", "number_of_replicas": 0}}}),
        ),
        (
            "ilm_index",
            "index",
            false,
            json!({"settings": {"index": {"lifecycle": {"name": "p1"}, "number_of_replicas": "0"}}}),
        ),
        (
            "rollover_alias_settings",
            "settings",
            false,
            json!({"settings": {"index.lifecycle.rollover_alias": "a", "index.number_of_replicas": "0"}}),
        ),
        (
            "tier_preference",
            "index",
            false,
            json!({"settings": {"index.routing.allocation.include._tier_preference": "data_hot", "index.number_of_replicas": "0"}}),
        ),
    ];
    let mut refused = vec![];
    for (case, ty, template, body) in &cases {
        if *template && es && (maj, min) < (7, 8) {
            continue;
        }
        let name = unique(&format!("l656_{case}"));
        let path = if *template {
            format!("_index_template/{name}")
        } else {
            name.clone()
        };
        let mut direct = body.clone();
        if *template {
            direct["index_patterns"] = json!([format!("{name}-*")]);
            direct["priority"] = json!(500);
        }
        let (st, resp) = c.call("PUT", &path, Some(direct.clone())).await;
        let _ = c.call("DELETE", &path, None).await;
        let file = tmp_path("l656", "json");
        std::fs::write(&file, json!({ &name: direct }).to_string() + "\n").unwrap();
        let out = if *template {
            c.url_with_auth("")
        } else {
            c.url_with_auth(&name)
        };
        let o = run_loud(io(&c, file.to_str().unwrap(), &out, &["--type", ty])).await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        let tool_refused = err.contains("nothing was written")
            && (err.contains("cannot take these") || err.contains("rejects ("));
        let (exists, _) = c.call("GET", &path, None).await;
        let _ = c.call("DELETE", &path, None).await;
        if tool_refused {
            assert_eq!(o.status.code(), Some(1), "{case}: {err}");
            assert_eq!(exists, 404, "{case}: refused, but it was created");
            assert!(
                st != 200 && missing(&resp),
                "{version}: the tool refuses {case}, but the cluster takes it: {st} {resp}"
            );
            refused.push(*case);
        } else {
            assert!(
                st == 200 || !missing(&resp),
                "{version}: the cluster rejects {case} ({resp}) but the tool did not refuse it: {err}"
            );
            if st == 200 {
                assert_success(&o);
            }
        }
    }
    println!("L-656 {version} oss={oss}: refused {refused:?}");

    // ILM policies: OSS build and OpenSearch refuse before any write
    let pol = tmp_path("l656_ilm", "json");
    let pname = unique("l656_pol");
    std::fs::write(
        &pol,
        json!({ &pname: {"policy": {"phases": {"delete": {"min_age": "30d", "actions": {"delete": {}}}}}} }).to_string() + "\n",
    )
    .unwrap();
    let o = run_loud(io(
        &c,
        pol.to_str().unwrap(),
        &c.url_with_auth(""),
        &["--type", "policy"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    if oss {
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains("ILM is an X-Pack feature and this is an OSS build"),
            "{err}"
        );
    } else if !es {
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(err.contains("--convertIlmToIsm"), "{err}");
    } else {
        assert_success(&o);
        let _ = c
            .call("DELETE", &format!("_ilm/policy/{pname}"), None)
            .await;
    }

    // PIT: no PIT on the OSS build and OpenSearch < 2.4: scroll, with a warning
    let src = unique("l656_src");
    seed_index(&c, &src, 3).await;
    let n = c.count(&src).await;
    let dump = tmp_path("l656_pit", "json");
    let o = run_loud(io(
        &c,
        &c.url_with_auth(&src),
        dump.to_str().unwrap(),
        &["--searchType", "pit"],
    ))
    .await;
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert_success(&o);
    assert_eq!(lines(&dump).len() as u64, n);
    let no_pit = oss || (!es && (maj, min) < (2, 4)) || (es && (maj, min) < (7, 10));
    assert_eq!(err.contains("using scroll instead"), no_pit, "{err}");

    // credentials sent to the OSS build are not checked by it: warned
    if oss && !c.url.starts_with("https") {
        let dst = unique("l656_auth");
        let url = c.url_with_auth(&dst).replacen("://", "://u:FAKE-pw@", 1);
        let doc = tmp_path("l656_auth", "json");
        std::fs::write(
            &doc,
            "{\"_index\":\"x\",\"_id\":\"1\",\"_source\":{\"n\":1}}\n",
        )
        .unwrap();
        let o = run_loud(io(&c, doc.to_str().unwrap(), &url, &[])).await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_success(&o);
        assert!(
            err.contains("which has no X-Pack security: it does not check the credentials"),
            "{err}"
        );
        assert!(!err.contains("FAKE-pw"), "{err}");
        c.delete_index(&dst).await;
    }
    c.delete_index(&src).await;

    // data streams (OpenSearch, Elasticsearch >= 7.9 with X-Pack): `index` fails with one hint
    if !oss && (!es || (maj, min) >= (7, 9)) {
        let ds = unique("l656_ds");
        c.ok(
            "PUT",
            &format!("_index_template/{ds}"),
            Some(json!({"index_patterns": [ds.clone()], "data_stream": {}, "priority": 500})),
        )
        .await;
        let data = tmp_path("l656_ds", "json");
        std::fs::write(
            &data,
            (1..=2)
                .map(|i| json!({"_index": ds, "_id": format!("{i}"), "_source": {"@timestamp": "2026-01-01T00:00:00Z", "n": i}}).to_string() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let o = run_loud(io(&c, data.to_str().unwrap(), &c.url_with_auth(&ds), &[])).await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert_eq!(o.status.code(), Some(3), "{err}");
        assert_eq!(
            err.matches("the target is a data stream, which accepts only `create`")
                .count(),
            1,
            "{err}"
        );
        let o = run_loud(io(
            &c,
            data.to_str().unwrap(),
            &c.url_with_auth(&ds),
            &["--bulkAction", "create"],
        ))
        .await;
        assert_success(&o);
        assert_eq!(c.count(&ds).await, 2);
        let _ = c.call("DELETE", &format!("_data_stream/{ds}"), None).await;
        let _ = c
            .call("DELETE", &format!("_index_template/{ds}"), None)
            .await;
    }
}
