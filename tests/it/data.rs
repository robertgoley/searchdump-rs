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

/// L4-003 `--pageBytes`: 120 documents of ~3 KB (incompressible) against a 48 KB cap. searchdump
/// sizes the first page from the index's average stored document (scroll and search_after,
/// `--serverLimits` auto and off) and keeps later pages under the cap; searchdump-multi records
/// the `page-bytes` sizing, passes `--pageBytes` to the data child, and every document arrives.
#[tokio::test]
async fn l4_003_page_bytes_caps_dump_pages() {
    let c = Cluster::from_env();
    let index = unique("pagebytes");
    c.delete_index(&index).await;
    c.ok(
        "PUT",
        &index,
        Some(json!({"settings": {"number_of_shards": 1, "number_of_replicas": 0}})),
    )
    .await;
    let mut nd = String::new();
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    for i in 0..120u64 {
        let mut pad = String::with_capacity(3000);
        while pad.len() < 3000 {
            // xorshift: incompressible, so stored size ~ source size
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            pad.push_str(&format!("{x:016x}"));
        }
        nd.push_str(&json!({"index": {"_index": &index, "_id": i.to_string()}}).to_string());
        nd.push('\n');
        nd.push_str(&json!({"n": i, "pad": pad}).to_string());
        nd.push('\n');
    }
    c.bulk_raw(nd).await;
    c.ok("POST", &format!("{index}/_flush"), None).await;
    let cap: u64 = 48 * 1024;
    for (label, extra) in [
        ("scroll-auto", vec![]),
        ("scroll-off", vec!["--serverLimits", "off"]),
        (
            "search-after",
            vec![
                "--searchAfter",
                "--searchBody",
                r#"{"query":{"match_all":{}},"sort":[{"n":"asc"}]}"#,
            ],
        ),
    ] {
        let out = tmp_path("pagebytes", "json");
        let mut a = s(&[
            "--input",
            &c.url_with_auth(&index),
            "--output",
            out.to_str().unwrap(),
        ]);
        a.extend(c.tls_args("input"));
        a.extend(s(&[
            "--pageBytes",
            &cap.to_string(),
            "--limit",
            "100",
            "--debug",
        ]));
        a.extend(s(&extra));
        let o = run_loud(a).await;
        let err = String::from_utf8_lossy(&o.stderr).into_owned();
        assert!(
            o.status.success(),
            "{label}: {}",
            err.lines().rev().take(30).collect::<Vec<_>>().join("\n")
        );
        assert_eq!(lines(&out).len(), 120, "{label}");
        let first = err
            .lines()
            .find(|l| l.contains("--pageBytes") && l.contains(" docs"))
            .unwrap_or_else(|| {
                panic!(
                    "{label}: no sizing line: {:?}",
                    err.lines()
                        .filter(|l| l.contains("limits") || l.contains("pageBytes"))
                        .collect::<Vec<_>>()
                )
            });
        // 48 KB / (~3 KB x 3) = 5 documents (stored size varies a little)
        let page: u64 = first
            .split("page ")
            .nth(1)
            .and_then(|r| r.split(' ').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or_else(|| panic!("{label}: {first}"));
        assert!((2..=8).contains(&page), "{label}: {first}");
        // every page read after the first is under the cap
        let reads: Vec<u64> = err
            .lines()
            .filter_map(|l| l.split("Read ").nth(1))
            .filter_map(|r| r.split(' ').next()?.parse().ok())
            .collect();
        if label == "search-after" {
            // no byte lines on this path: every page asks for at most the sized page
            let pages: Vec<u64> = err
                .lines()
                .filter_map(|l| l.split("search_after page with ").nth(1))
                .filter_map(|r| r.split(' ').next()?.parse().ok())
                .collect();
            assert!(pages.len() >= 120 / page as usize, "{label}: {pages:?}");
            assert!(pages.iter().all(|&n| n <= page), "{label}: {pages:?}");
        } else {
            assert!(!reads.is_empty(), "{label}: no read lines");
            assert!(
                reads.iter().skip(1).all(|&n| n <= cap),
                "{label}: {reads:?}"
            );
        }
        let _ = std::fs::remove_file(&out);
    }

    // searchdump-multi: sizing recorded, flag passed to the data child
    let dir = tmp_path("pagebytes_multi", "d");
    let logs = tmp_path("pagebytes_multi_logs", "d");
    let mut a = s(&[
        "--direction",
        "dump",
        "--input",
        &c.url_with_auth(""),
        "--output",
        dir.to_str().unwrap(),
        "--match",
        &format!("^{index}$"),
        "--logDir",
        logs.to_str().unwrap(),
        "--pageBytes",
        &cap.to_string(),
    ]);
    a.extend(c.tls_args("input"));
    let o = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_searchdump-multi"))
            .args(&a)
            .env("SEARCHDUMP_BIN", bin())
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{err}");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("multielasticdump.json")).unwrap())
            .unwrap();
    let sizing = &manifest["indices"][&index]["sizing"];
    assert_eq!(sizing["reason"], "page-bytes", "{manifest}");
    let limit = sizing["limit"].as_u64().unwrap();
    assert!((2..=8).contains(&limit), "{sizing}");
    let log = std::fs::read_to_string(logs.join(format!("{index}.data.log"))).unwrap_or_default();
    assert!(
        !log.contains("above --pageBytes") || log.contains("asking for"),
        "{log}"
    );
    let data: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.to_string_lossy().contains(".data")
                || p.to_string_lossy().ends_with(".json") && p.to_string_lossy().contains(&index)
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(!data.is_empty(), "{data:?}");
    c.delete_index(&index).await;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&logs);
}
