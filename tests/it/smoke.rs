use crate::harness::*;

/// M0 smoke test: a full scroll dump of a seeded 2-shard index returns every
/// document.
#[tokio::test]
async fn scroll_dump_returns_all_docs() {
    let c = Cluster::from_env();
    let index = unique("smoke");
    seed_index(&c, &index, 500).await;
    let out = tmp_path("smoke", "json");
    let mut args = s(&[
        "--input",
        &c.url_with_auth(&index),
        "--output",
        out.to_str().unwrap(),
    ]);
    args.extend(c.tls_args("input"));
    let o = run(args).await;
    assert_success(&o);
    assert_eq!(lines(&out).len() as u64, c.count(&index).await);
    c.delete_index(&index).await;
    let _ = std::fs::remove_file(out);
}

/// OpenSearch PIT (flavor-aware open/close and `_doc`+`_id` sort), unsliced
/// and sliced, returns every document exactly once.
#[tokio::test]
async fn pit_dump_returns_all_docs_unsliced_and_sliced() {
    let c = Cluster::from_env();
    let index = unique("pit");
    seed_index(&c, &index, 1000).await;
    let expected = c.count(&index).await;
    for extra in [
        vec!["--searchAfter", "--pit"],
        vec![
            "--searchType",
            "pit",
            "--slices",
            "3",
            "--sliceCap=false",
            "--limit",
            "97",
        ],
    ] {
        let out = tmp_path("pit", "json");
        let mut args = s(&[
            "--input",
            &c.url_with_auth(&index),
            "--output",
            out.to_str().unwrap(),
        ]);
        args.extend(s(&extra));
        args.extend(c.tls_args("input"));
        let o = run(args).await;
        assert_success(&o);
        let ids: std::collections::BTreeSet<String> = lines(&out)
            .iter()
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).unwrap()["_id"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(lines(&out).len() as u64, expected, "{extra:?}");
        assert_eq!(ids.len() as u64, expected, "duplicates with {extra:?}");
        let _ = std::fs::remove_file(out);
    }
    c.delete_index(&index).await;
}

/// `--size` and `--offset` against a real cluster.
#[tokio::test]
async fn size_and_offset() {
    let c = Cluster::from_env();
    let index = unique("size");
    seed_index(&c, &index, 300).await;
    let out = tmp_path("size", "json");
    let mut args = s(&[
        "--input",
        &c.url_with_auth(&index),
        "--output",
        out.to_str().unwrap(),
        "--size",
        "123",
        "--offset",
        "10",
        "--limit",
        "50",
    ]);
    args.extend(c.tls_args("input"));
    let o = run(args).await;
    assert_success(&o);
    assert_eq!(lines(&out).len(), 123);
    let _ = std::fs::remove_file(out);
    c.delete_index(&index).await;
}
