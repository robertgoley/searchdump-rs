//! M2: restore (file → cluster), cluster → cluster, search modes, size,
//! scrollId resume, bulk error handling.

use serde_json::{Value, json};

use crate::harness::*;

fn io_args(c: &Cluster, input: &str, output: &str) -> Vec<String> {
    let mut a = s(&["--input", input, "--output", output]);
    if input.starts_with("http") {
        a.extend(c.tls_args("input"));
    }
    if output.starts_with("http") {
        a.extend(c.tls_args("output"));
    }
    a
}

async fn dump_to_file(c: &Cluster, index: &str, extra: &[&str]) -> std::path::PathBuf {
    let out = tmp_path("dump", "json");
    let mut a = io_args(c, &c.url_with_auth(index), out.to_str().unwrap());
    a.extend(s(extra));
    assert_success(&run(a).await);
    out
}

fn ids(path: &std::path::Path) -> Vec<String> {
    lines(path)
        .iter()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["_id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

#[tokio::test]
async fn file_restore_round_trip_is_lossless() {
    let c = Cluster::from_env();
    let src = unique("rt_src");
    let dst = unique("rt_dst");
    seed_index(&c, &src, 10_000).await;
    let file = dump_to_file(&c, &src, &[]).await;
    c.copy_shape(&src, &dst).await;

    let mut a = io_args(&c, file.to_str().unwrap(), &c.url_with_auth(&dst));
    a.extend(s(&["--limit", "250"]));
    let o = run(a).await;
    assert_success(&o);

    assert_eq!(
        c.docs(&src).await,
        c.docs(&dst).await,
        "ids, routing and sources"
    );
    // big integers and unicode survive byte-for-byte
    let back = dump_to_file(&c, &dst, &[]).await;
    assert_eq!(raw_sources_by_id(&file), raw_sources_by_id(&back));

    for i in [&src, &dst] {
        c.delete_index(i).await;
    }
    let _ = std::fs::remove_file(file);
    let _ = std::fs::remove_file(back);
}

#[tokio::test]
async fn cluster_to_cluster_with_prefix_suffix_and_gzip_requests() {
    let c = Cluster::from_env();
    let src = unique("c2c_src");
    seed_index(&c, &src, 600).await;
    let dst = format!("pre_{src}_suf");
    c.copy_shape(&src, &dst).await;
    let mut a = io_args(&c, &c.url_with_auth(&src), &c.url_with_auth(&src));
    a.extend(s(&[
        "--prefix",
        "pre_",
        "--suffix",
        "_suf",
        "--esCompress",
        "--limit",
        "128",
    ]));
    let o = run(a).await;
    assert_success(&o);
    assert_eq!(c.docs(&src).await, c.docs(&dst).await);
    c.delete_index(&src).await;
    c.delete_index(&dst).await;
}

#[tokio::test]
async fn host_only_input_with_output_index_and_pit_slices() {
    let c = Cluster::from_env();
    let src = unique("pitc2c");
    let dst = unique("pitc2c_dst");
    seed_index(&c, &src, 700).await;
    c.copy_shape(&src, &dst).await;
    let mut a = io_args(&c, &c.url_with_auth(&src), &c.url_with_auth(""));
    a.extend(s(&[
        "--output-index",
        &dst,
        "--searchType",
        "pit",
        "--slices",
        "2",
        "--sliceCap=false",
        "--noRefresh",
    ]));
    let o = run(a).await;
    assert_success(&o);
    assert_eq!(c.docs(&src).await, c.docs(&dst).await);
    c.delete_index(&src).await;
    c.delete_index(&dst).await;
}

#[tokio::test]
async fn exact_size_with_scroll_and_sliced_pit() {
    let c = Cluster::from_env();
    let src = unique("size1234");
    seed_index(&c, &src, 1500).await;
    for extra in [
        vec!["--limit", "500"],
        vec![
            "--searchType",
            "pit",
            "--slices",
            "3",
            "--sliceCap=false",
            "--limit",
            "200",
        ],
    ] {
        let mut e = extra.clone();
        e.extend(["--size", "1234"]);
        let f = dump_to_file(&c, &src, &e).await;
        let got = ids(&f);
        assert_eq!(got.len(), 1234, "{extra:?}");
        let uniq: std::collections::BTreeSet<_> = got.iter().collect();
        assert_eq!(uniq.len(), 1234, "{extra:?}");
        let _ = std::fs::remove_file(f);
    }
    c.delete_index(&src).await;
}

#[tokio::test]
async fn search_after_without_pit_requires_sort_and_returns_all() {
    let c = Cluster::from_env();
    let src = unique("sa");
    seed_index(&c, &src, 450).await;
    let out = tmp_path("sa", "json");
    let mut a = io_args(&c, &c.url_with_auth(&src), out.to_str().unwrap());
    a.extend(s(&["--searchAfter"]));
    let o = run(a.clone()).await;
    assert_eq!(o.status.code(), Some(2), "sort is required");

    a.extend(s(&[
        "--searchBody",
        r#"{"query":{"match_all":{}},"sort":[{"k":"asc"}]}"#,
        "--limit",
        "100",
    ]));
    let o = run(a).await;
    assert_success(&o);
    let got = ids(&out);
    assert_eq!(got.len() as u64, c.count(&src).await);
    let uniq: std::collections::BTreeSet<_> = got.iter().collect();
    assert_eq!(uniq.len(), got.len());
    c.delete_index(&src).await;
    let _ = std::fs::remove_file(out);
}

#[tokio::test]
async fn scroll_id_resumes_an_open_scroll() {
    let c = Cluster::from_env();
    let src = unique("resume");
    seed_index(&c, &src, 250).await;
    let total = c.count(&src).await;
    let first = c
        .ok(
            "POST",
            &format!("{src}/_search?scroll=5m"),
            Some(json!({"size": 100, "sort": ["_doc"]})),
        )
        .await;
    let sid = first["_scroll_id"].as_str().unwrap().to_string();
    let out = tmp_path("resume", "json");
    let mut a = io_args(&c, &c.url_with_auth(&src), out.to_str().unwrap());
    a.extend(s(&["--scrollId", &sid]));
    let o = run(a).await;
    assert_success(&o);
    assert_eq!(lines(&out).len() as u64, total - 100);
    c.delete_index(&src).await;
    let _ = std::fs::remove_file(out);
}

#[tokio::test]
async fn typed_6x_fixture_restores_typeless() {
    let c = Cluster::from_env();
    let dst = unique("typed");
    let file = tmp_path("typed", "json");
    std::fs::write(
        &file,
        "{\"_index\":\"old\",\"_type\":\"doc\",\"_id\":\"1\",\"_routing\":\"r\",\"_source\":{\"a\":1}}\n{\"_index\":\"old\",\"_type\":\"doc\",\"_id\":\"2\",\"_source\":{\"a\":2}}\n",
    )
    .unwrap();
    let a = io_args(&c, file.to_str().unwrap(), &c.url_with_auth(&dst));
    assert_success(&run(a).await);
    let docs = c.docs(&dst).await;
    assert_eq!(docs.len(), 2);
    assert_eq!(docs["1"], (Some("r".to_string()), json!({"a": 1})));
    c.delete_index(&dst).await;
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn bulk_item_errors_respect_ignore_flags() {
    let c = Cluster::from_env();
    let dst = unique("strict");
    c.ok(
        "PUT",
        &dst,
        Some(json!({"mappings": {"dynamic": "strict", "properties": {"a": {"type": "integer"}}}})),
    )
    .await;
    let file = tmp_path("strict", "json");
    std::fs::write(
        &file,
        "{\"_id\":\"1\",\"_source\":{\"a\":1}}\n{\"_id\":\"2\",\"_source\":{\"b\":2}}\n{\"_id\":\"3\",\"_source\":{\"a\":3}}\n",
    )
    .unwrap();

    // default: the run continues past the item error, then exits 3 (L-444, Q3)
    let mut a = io_args(&c, file.to_str().unwrap(), &c.url_with_auth(&dst));
    a.push("--noImplicitRetry".into());
    let o = run_env(a.clone(), vec![]).await;
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("strict_dynamic_mapping_exception"), "{err}");
    assert_eq!(c.count(&dst).await, 2);
    // explicit --ignore-es-write-errors: exit 0 (Node's default)
    let mut b = a.clone();
    b.push("--ignore-es-write-errors".into());
    assert_success(&run_env(b, vec![]).await);

    // ignore-es-write-errors=false → exit 1
    a.extend(s(&["--ignore-es-write-errors", "false"]));
    let o = run(a).await;
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("bulk write failed"));
    c.delete_index(&dst).await;
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn source_only_dump_restores_with_new_ids() {
    let c = Cluster::from_env();
    let src = unique("srconly");
    let dst = unique("srconly_dst");
    seed_index(&c, &src, 120).await;
    let f = dump_to_file(&c, &src, &["--sourceOnly"]).await;
    c.copy_shape(&src, &dst).await;
    let first: Value = serde_json::from_str(&lines(&f)[0]).unwrap();
    assert!(first.get("_id").is_none() && first.get("k").is_some());
    let a = io_args(&c, f.to_str().unwrap(), &c.url_with_auth(&dst));
    assert_success(&run(a).await);
    assert_eq!(c.count(&dst).await, c.count(&src).await);
    c.delete_index(&src).await;
    c.delete_index(&dst).await;
    let _ = std::fs::remove_file(f);
}

/// L-455 (TST-BRK-XV-002): an index with `_source.enabled: false` cannot be fully dumped:
/// refused by default, written without _source with --allowMissingSource.
#[tokio::test]
async fn xv_source_disabled_index_is_refused_unless_allowed() {
    let c = Cluster::from_env();
    let idx = unique("nosrc");
    let (mapping, doc) = if c.engine == Engine::Elasticsearch && c.major().await < 7 {
        (
            json!({"doc": {"_source": {"enabled": false}}}),
            format!("{idx}/doc/1?refresh=true"),
        )
    } else {
        (
            json!({"_source": {"enabled": false}}),
            format!("{idx}/_doc/1?refresh=true"),
        )
    };
    c.ok("PUT", &idx, Some(json!({"mappings": mapping}))).await;
    c.ok("PUT", &doc, Some(json!({"a": 1}))).await;
    let out = tmp_path("nosrc", "json");
    let o = run(io_args(&c, &c.url_with_auth(&idx), out.to_str().unwrap())).await;
    let e = String::from_utf8_lossy(&o.stderr).into_owned();
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(e.contains("documents cannot be fully dumped"), "{e}");
    assert!(!out.exists());
    let mut a = io_args(&c, &c.url_with_auth(&idx), out.to_str().unwrap());
    a.push("--allowMissingSource".into());
    assert_success(&run(a).await);
    let l = lines(&out);
    assert_eq!(l.len(), 1);
    let v: Value = serde_json::from_str(&l[0]).unwrap();
    assert_eq!(v["_id"], "1");
    assert!(v.get("_source").is_none(), "{v}");
    c.delete_index(&idx).await;
}

/// L-456 (TST-BRK-XV-003): a data stream dumped to a file is restored into the stream with
/// --bulkAction create; with the default action the restore stops before writing.
#[tokio::test]
async fn xv_data_stream_restores_into_the_stream_with_create() {
    let c = Cluster::from_env();
    let stream = unique("logs-xv");
    let (st, _) = c
        .call(
            "PUT",
            &format!("_index_template/{stream}"),
            Some(json!({"index_patterns": [format!("{stream}*")], "data_stream": {}, "priority": 900})),
        )
        .await;
    if st >= 300 {
        println!("SKIP: no data streams on {} ({st})", c.version().await);
        return;
    }
    for n in 0..2 {
        c.ok(
            "POST",
            &format!("{stream}/_doc?refresh=true"),
            Some(json!({"@timestamp": format!("2026-10-30T00:00:0{n}Z"), "n": n})),
        )
        .await;
    }
    let out = tmp_path("ds", "json");
    assert_success(
        &run(io_args(
            &c,
            &c.url_with_auth(&stream),
            out.to_str().unwrap(),
        ))
        .await,
    );
    let l = lines(&out);
    assert_eq!(l.len(), 2);
    assert!(l.iter().all(|x| x.contains("\"_index\":\".ds-")), "{l:?}");
    c.ok("DELETE", &format!("_data_stream/{stream}"), None)
        .await;

    let o = run(io_args(&c, out.to_str().unwrap(), &c.url_with_auth(""))).await;
    let e = String::from_utf8_lossy(&o.stderr).into_owned();
    assert_eq!(o.status.code(), Some(1), "{e}");
    assert!(e.contains(&format!("data stream '{stream}'")), "{e}");
    assert_eq!(
        c.call("GET", &format!("_data_stream/{stream}"), None)
            .await
            .0,
        404
    );
    let (bs, _) = c.call("GET", &format!(".ds-{stream}*/_search"), None).await;
    let hidden = c
        .call(
            "GET",
            &format!("_cat/indices/.ds-{stream}*?format=json&expand_wildcards=all"),
            None,
        )
        .await
        .1;
    assert!(
        hidden.as_array().is_none_or(|a| a.is_empty()),
        "no plain backing-named index: {bs} {hidden}"
    );

    let mut a = io_args(&c, out.to_str().unwrap(), &c.url_with_auth(""));
    a.extend(s(&["--bulkAction", "create"]));
    assert_success(&run(a).await);
    c.ok("POST", &format!("{stream}/_refresh"), None).await;
    let ds = c.ok("GET", &format!("_data_stream/{stream}"), None).await;
    assert_eq!(ds["data_streams"][0]["name"], stream.as_str(), "{ds}");
    assert_eq!(c.count(&stream).await, 2);
    c.ok("DELETE", &format!("_data_stream/{stream}"), None)
        .await;
    c.ok("DELETE", &format!("_index_template/{stream}"), None)
        .await;
}
