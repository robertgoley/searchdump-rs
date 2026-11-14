//! CLI contract tests that need no cluster: flag handling, exit codes, file
//! transfers and request shaping against an in-process mock server.

mod support;

use std::process::{Command, Output, Stdio};

use support::mock_es::{CannedResponse, MockEs};

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_searchdump"));
    c.env_remove("SEARCHDUMP_INPUT_USERNAME")
        .env_remove("SEARCHDUMP_INPUT_PASSWORD")
        .env_remove("SEARCHDUMP_OUTPUT_USERNAME")
        .env_remove("SEARCHDUMP_OUTPUT_PASSWORD")
        .env_remove("ELASTICDUMP_INPUT_USERNAME")
        .env_remove("ELASTICDUMP_INPUT_PASSWORD")
        .env_remove("ELASTICDUMP_OUTPUT_USERNAME")
        .env_remove("ELASTICDUMP_OUTPUT_PASSWORD")
        .env_remove("NODE_TLS_REJECT_UNAUTHORIZED")
        .env_remove("RUST_LOG");
    c
}

fn run(args: &[&str]) -> Output {
    bin().args(args).output().expect("spawn")
}

async fn run_async(args: Vec<String>, envs: Vec<(String, String)>) -> Output {
    tokio::task::spawn_blocking(move || bin().args(args).envs(envs).output().expect("spawn"))
        .await
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn hit(id: &str, src: &str) -> String {
    format!(r#"{{"_index":"idx","_id":"{id}","_score":1.0,"_source":{src}}}"#)
}

fn page(scroll_id: &str, hits: &[String], total: usize) -> String {
    format!(
        r#"{{"_scroll_id":"{scroll_id}","timed_out":false,"_shards":{{"total":1,"successful":1,"skipped":0,"failed":0}},"hits":{{"total":{{"value":{total},"relation":"eq"}},"hits":[{}]}}}}"#,
        hits.join(",")
    )
}

/// initial page with `hits`, then an empty continuation, then clear-scroll.
fn scroll_script(hits: &[String]) -> Vec<CannedResponse> {
    vec![
        CannedResponse::ok(page("s1", hits, hits.len())),
        CannedResponse::ok(page("s1", &[], hits.len())),
        CannedResponse::ok(r#"{"succeeded":true,"num_freed":1}"#),
    ]
}

#[test]
fn unknown_type_and_file_to_file_metadata_are_usage_errors() {
    let o = run(&["--input", "in.json", "--output", "$", "--type", "bogus"]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("index_template"), "{}", stderr(&o));
    let o = run(&[
        "--input", "in.json", "--output", "out.json", "--type", "alias",
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    assert!(stderr(&o).contains("at least one side"), "{}", stderr(&o));
}

#[test]
fn out_of_scope_data_changing_flags_are_errors() {
    for flag in [
        "--transform=doc._source.x=1",
        "--awsChain",
        "--searchBodyTemplate=x.js",
        "--delete",
    ] {
        let o = run(&["--input", "in.json", "--output", "$", flag]);
        assert_eq!(o.status.code(), Some(2), "{flag}: {}", stderr(&o));
        assert!(
            stderr(&o).contains("not supported"),
            "{flag}: {}",
            stderr(&o)
        );
    }
}

#[test]
fn out_of_scope_tuning_flags_warn_and_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(&input, "{\"a\":1}\n").unwrap();
    let o = run(&[
        "--input",
        input.to_str().unwrap(),
        "--output",
        "$",
        "--s3Region",
        "us-east-1",
        "--csvDelimiter=;",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "{\"a\":1}\n");
    assert!(stderr(&o).contains("Ignoring --s3Region"), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("Ignoring --csvDelimiter"),
        "{}",
        stderr(&o)
    );
}

#[test]
fn split_flags_need_a_file_output() {
    let o = run(&["--input", "in.json", "--output", "$", "--fileSize", "1mb"]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
    let o = run(&[
        "--input",
        "in.json",
        "--output",
        "out.json",
        "--fileSize",
        "1mb",
        "--maxRows",
        "5",
    ]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
}

#[test]
fn existing_output_without_overwrite_fails() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    let out = dir.path().join("out.json");
    std::fs::write(&input, "{\"a\":1}\n").unwrap();
    std::fs::write(&out, "keep\n").unwrap();
    let o = run(&[
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("already exists"), "{}", stderr(&o));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "keep\n");
    let o = run(&[
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--overwrite",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "{\"a\":1}\n");
}

/// L-805a: gzip 9 and zstd 19+ warn on stderr (still used, exit 0); defaults do not.
#[test]
fn slow_compression_levels_warn_but_still_run() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(&input, "{\"_id\":\"1\",\"_source\":{\"n\":1}}\n").unwrap();
    let cases: [(&[&str], Option<&str>); 5] = [
        (
            &["--fsCompress", "--compressionLevel", "9"],
            Some("--compressionLevel 9 with gzip is slow"),
        ),
        (
            &["--compressionType", "zstd", "--compressionLevel", "19"],
            Some("--compressionLevel 19 with zstd is very slow"),
        ),
        (&["--fsCompress"], None),
        (&["--compressionType", "zstd"], None),
        (
            &["--compressionType", "zstd", "--compressionLevel", "9"],
            None,
        ),
    ];
    for (i, (flags, want)) in cases.iter().enumerate() {
        let out = dir.path().join(format!("out{i}.json"));
        let mut args = vec![
            "--input",
            input.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(flags);
        let o = run(&args);
        let e = stderr(&o);
        assert!(o.status.success(), "{flags:?}: {e}");
        assert!(e.contains("Total Writes: 1"), "{flags:?}: {e}");
        match want {
            Some(w) => assert!(
                e.contains(w) && e.contains("README compression table"),
                "{flags:?}: {e}"
            ),
            None => assert!(!e.contains("README compression table"), "{flags:?}: {e}"),
        }
    }
}

/// L-805a / D-5: an `--overwrite` run names a killed run's hidden leftovers, says that the
/// overwrite does not remove them, and leaves them in place.
#[test]
fn overwrite_names_leftovers_and_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    let out = dir.path().join("out.json");
    std::fs::write(&input, "{\"a\":1}\n").unwrap();
    std::fs::write(&out, "old\n").unwrap();
    // PID 4000000000 cannot be running, so no "still running" note
    let left = dir.path().join(".out.json.part-4000000000-1");
    std::fs::write(&left, "staged\n").unwrap();
    let args = [
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--overwrite",
    ];
    let o = run(&args);
    let e = stderr(&o);
    assert!(o.status.success(), "{e}");
    assert!(
        e.contains(".out.json.part-4000000000-1") && e.contains("left in place"),
        "{e}"
    );
    assert!(
        e.contains("--overwrite replaces only the output itself; it does not remove these files"),
        "{e}"
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "{\"a\":1}\n");
    assert_eq!(std::fs::read_to_string(&left).unwrap(), "staged\n");
    // without --overwrite (new output name) the note is not added
    let out2 = dir.path().join("out2.json");
    std::fs::write(dir.path().join(".out2.json.part-4000000000-2"), "x").unwrap();
    let o = run(&[
        "--input",
        input.to_str().unwrap(),
        "--output",
        out2.to_str().unwrap(),
    ]);
    let e = stderr(&o);
    assert!(o.status.success(), "{e}");
    assert!(
        e.contains("left in place") && !e.contains("--overwrite replaces only"),
        "{e}"
    );
}

#[test]
fn file_to_file_normalises_concatenated_and_pretty_json_with_offset_and_size() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        "{\"_id\":\"1\",\"_source\":{\"n\":1}}{\"_id\":\"2\",\"_source\":{\"n\":2}}\n{\n  \"_id\": \"3\",\n  \"_source\": {\"n\": 123456789012345678901234567890}\n}\n{\"_id\":\"4\",\"_source\":{}}\n",
    )
    .unwrap();
    let out = dir.path().join("out.json");
    let o = run(&[
        "--input",
        input.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--limit",
        "1",
        "--offset",
        "1",
        "--size",
        "2",
    ]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "{\"_id\":\"2\",\"_source\":{\"n\":2}}\n{\"_id\":\"3\",\"_source\":{\"n\":123456789012345678901234567890}}\n"
    );
    assert!(stderr(&o).contains("Total Writes: 2"), "{}", stderr(&o));
}

#[test]
fn stdin_to_stdout() {
    let mut child = bin()
        .args(["--input", "$", "--output", "$", "--quiet"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"{\"a\":1}\n{\"b\":2}\n").unwrap();
    }
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success());
    assert_eq!(stdout(&o), "{\"a\":1}\n{\"b\":2}\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn headers_and_params_reach_every_request() {
    let mock = MockEs::serve(scroll_script(&[hit("1", "{\"a\":1}")])).await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
            "--headers".into(),
            r#"{"X-Global":"g","X-Over":"global"}"#.into(),
            "--input-headers".into(),
            r#"{"X-Over":"input"}"#.into(),
            "--input-params".into(),
            r#"{"preference":"_local"}"#.into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3, "{reqs:#?}");
    for r in &reqs {
        let lower = r.to_ascii_lowercase();
        assert!(lower.contains("\r\nx-global: g\r\n"), "{r}");
        assert!(lower.contains("\r\nx-over: input\r\n"), "{r}");
        assert!(lower.contains("\r\nuser-agent: elasticdump\r\n"), "{r}");
    }
    assert!(reqs[0].starts_with("POST /idx/_search?"), "{}", reqs[0]);
    assert!(reqs[0].contains("scroll=10m"), "{}", reqs[0]);
    assert!(reqs[0].contains("preference=_local"), "{}", reqs[0]);
    assert!(reqs[1].starts_with("POST /_search/scroll "), "{}", reqs[1]);
    assert!(
        reqs[2].starts_with("DELETE /_search/scroll "),
        "{}",
        reqs[2]
    );
    assert_eq!(stdout(&o), format!("{}\n", hit("1", "{\"a\":1}")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_only_writes_raw_sources() {
    let mock = MockEs::serve(scroll_script(&[
        hit("1", "{\"n\":12345678901234567890}"),
        hit("2", "{\"s\":\"\\u00e9\"}"),
    ]))
    .await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
            "--sourceOnly".into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(
        stdout(&o),
        "{\"n\":12345678901234567890}\n{\"s\":\"\\u00e9\"}\n"
    );
}

fn basic(user: &str, pass: &str) -> String {
    use base64::prelude::*;
    format!(
        "authorization: basic {}",
        BASE64_STANDARD.encode(format!("{user}:{pass}"))
    )
    .to_ascii_lowercase()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn credentials_from_auth_file_env_and_url() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.ini");
    std::fs::write(&auth, "user=fileuser\npassword=filepass\n").unwrap();

    // auth file
    let mock = MockEs::serve(scroll_script(&[])).await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
            "--httpAuthFile".into(),
            auth.to_str().unwrap().into(),
        ],
        vec![
            ("SEARCHDUMP_INPUT_USERNAME".into(), "envuser".into()),
            ("SEARCHDUMP_INPUT_PASSWORD".into(), "envpass".into()),
        ],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[0]
            .to_ascii_lowercase()
            .contains(&basic("fileuser", "filepass"))
    );

    // env vars
    let mock = MockEs::serve(scroll_script(&[])).await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
        ],
        vec![
            ("SEARCHDUMP_INPUT_USERNAME".into(), "envuser".into()),
            ("SEARCHDUMP_INPUT_PASSWORD".into(), "envpass".into()),
        ],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[0]
            .to_ascii_lowercase()
            .contains(&basic("envuser", "envpass"))
    );

    // L-810a: the old ELASTICDUMP_* names still work (deprecated, with a warning that does not
    // show the value); the new SEARCHDUMP_* name wins when both are set
    let mock = MockEs::serve(scroll_script(&[])).await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
        ],
        vec![
            ("ELASTICDUMP_INPUT_USERNAME".into(), "olduser".into()),
            (
                "ELASTICDUMP_INPUT_PASSWORD".into(),
                "FAKE-oldpass-l810".into(),
            ),
        ],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[0]
            .to_ascii_lowercase()
            .contains(&basic("olduser", "FAKE-oldpass-l810"))
    );
    assert!(
        stderr(&o)
            .contains("ELASTICDUMP_INPUT_USERNAME is deprecated; use SEARCHDUMP_INPUT_USERNAME"),
        "{}",
        stderr(&o)
    );
    assert!(!stderr(&o).contains("FAKE-oldpass-l810"));
    let mock = MockEs::serve(scroll_script(&[])).await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
        ],
        vec![
            ("SEARCHDUMP_INPUT_USERNAME".into(), "newuser".into()),
            ("SEARCHDUMP_INPUT_PASSWORD".into(), "newpass".into()),
            ("ELASTICDUMP_INPUT_USERNAME".into(), "olduser".into()),
            ("ELASTICDUMP_INPUT_PASSWORD".into(), "oldpass".into()),
        ],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[0]
            .to_ascii_lowercase()
            .contains(&basic("newuser", "newpass"))
    );

    // URL credentials win over env
    let mock = MockEs::serve(scroll_script(&[])).await;
    let url = mock
        .input_url("idx")
        .replace("http://", "http://urluser:url%40pass@");
    let o = run_async(
        vec![
            "--input".into(),
            url,
            "--output".into(),
            "$".into(),
            "--quiet".into(),
        ],
        vec![("SEARCHDUMP_INPUT_USERNAME".into(), "envuser".into())],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[0]
            .to_ascii_lowercase()
            .contains(&basic("urluser", "url@pass"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_auth_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.ini");
    std::fs::write(&auth, "user=only\n").unwrap();
    let o = run_async(
        vec![
            "--input".into(),
            "http://127.0.0.1:9/idx".into(),
            "--output".into(),
            "$".into(),
            "--httpAuthFile".into(),
            auth.to_str().unwrap().into(),
        ],
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("Malformed Auth File"), "{}", stderr(&o));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn size_stops_early_and_clears_scroll() {
    let hits: Vec<String> = (1..=5).map(|i| hit(&i.to_string(), r#"{"n":1}"#)).collect();
    // The reader may or may not issue one continuation before it observes
    // the --size cancellation; an empty page keeps both orders valid.
    let mock = MockEs::serve(vec![
        CannedResponse::ok(page("s1", &hits, 50)),
        CannedResponse::ok(page("s1", &[], 50)),
        CannedResponse::ok(r#"{"succeeded":true,"num_freed":1}"#),
    ])
    .await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--offset".into(),
            "1".into(),
            "--size".into(),
            "3".into(),
            "--limit".into(),
            "5".into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let ids: Vec<String> = stdout(&o)
        .lines()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["_id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(ids, vec!["2", "3", "4"]);
    assert!(stderr(&o).contains("Total Writes: 3"), "{}", stderr(&o));
    let reqs = mock.requests();
    assert!(
        reqs.iter().any(|r| r.starts_with("DELETE /_search/scroll")),
        "{reqs:#?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_requests_are_shaped_like_elasticdump() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        format!("{}\n{}\n", hit("1", "{\"a\":1}"), hit("2", "{\"a\":2}")),
    )
    .unwrap();
    let mock = MockEs::serve(vec![
        CannedResponse::ok(r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#),
        CannedResponse::ok(r#"{"took":1,"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}}]}"#),
        CannedResponse::ok(r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#),
    ])
    .await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            mock.input_url("dst"),
            "--esCompress".into(),
            "--output-params".into(),
            r#"{"pipeline":"p1"}"#.into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3, "{reqs:#?}");
    assert!(reqs[0].starts_with("GET / "), "{}", reqs[0]);
    assert!(
        reqs[1].starts_with("POST /dst/_bulk?pipeline=p1 "),
        "{}",
        reqs[1]
    );
    let bulk = reqs[1].to_ascii_lowercase();
    assert!(
        bulk.contains("content-type: application/x-ndjson"),
        "{bulk}"
    );
    assert!(bulk.contains("content-encoding: gzip"), "{bulk}");
    assert!(reqs[2].starts_with("POST /dst/_refresh "), "{}", reqs[2]);
    assert!(stderr(&o).contains("Total Writes: 2"), "{}", stderr(&o));
}

// ------------------------------------------------ metadata types (M7-M12)

const ES8: &str = r#"{"version":{"number":"8.6.1"}}"#;
const OS2: &str = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;

fn write_tmp(name: &str, content: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("edrs-contract-{}-{name}", std::process::id()));
    std::fs::write(&p, content).unwrap();
    p
}

fn host(mock: &MockEs) -> String {
    mock.input_url("").trim_end_matches('/').to_string()
}

async fn meta_run(args: &[&str]) -> Output {
    let mut v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    v.extend(["--noImplicitRetry".to_string()]);
    run_async(v, vec![]).await
}

#[tokio::test]
async fn alias_restore_is_one_atomic_post_with_prefixed_targets() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::ok(r#"{"acknowledged":true}"#),
    ])
    .await;
    // elasticdump's own fixture (test/test-resources/alias.json) plus an index without aliases
    let f = write_tmp(
        "alias.json",
        r#""{\"source_index\":{\"aliases\":{\"the-source\":{},\"test-2018\":{\"is_write_index\":true}}},\"bare\":{\"aliases\":{}}}""#,
    );
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "alias",
        "--prefix",
        "p_",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[1].starts_with("POST /_aliases "), "{}", reqs[1]);
    assert!(
        reqs[1].contains(r#"{"actions":[{"add":{"index":"p_source_index","alias":"the-source"}},{"add":{"index":"p_source_index","alias":"test-2018","is_write_index":true}}]}"#),
        "{}",
        reqs[1]
    );
}

#[tokio::test]
async fn ism_overwrite_uses_sequence_numbers() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(OS2),
        CannedResponse::new(
            409,
            r#"{"error":{"type":"version_conflict_engine_exception"},"status":409}"#,
        ),
        CannedResponse::ok(r#"{"_id":"p1","_seq_no":7,"_primary_term":3,"policy":{"states":[]}}"#),
        CannedResponse::ok(r#"{"_id":"p1"}"#),
    ])
    .await;
    let f = write_tmp(
        "ism.json",
        r#"{"policies":[{"_id":"p1","_seq_no":1,"_primary_term":1,"policy":{"policy_id":"p1","last_updated_time":1,"schema_version":21,"default_state":"a","states":[{"name":"a","actions":[],"transitions":[]}],"ism_template":[{"index_patterns":["x-*"],"priority":1,"last_updated_time":2}]}}],"total_policies":1}"#,
    );
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "policy",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let reqs = mock.requests();
    assert!(
        reqs[1].starts_with("PUT /_plugins/_ism/policies/p1 "),
        "{}",
        reqs[1]
    );
    assert!(
        !reqs[1].contains("last_updated_time") && !reqs[1].contains("policy_id"),
        "{}",
        reqs[1]
    );
    assert!(
        reqs[2].starts_with("GET /_plugins/_ism/policies/p1 "),
        "{}",
        reqs[2]
    );
    assert!(
        reqs[3].starts_with("PUT /_plugins/_ism/policies/p1?if_seq_no=7&if_primary_term=3 "),
        "{}",
        reqs[3]
    );
}

#[tokio::test]
async fn ism_list_dump_pages_through_all_policies() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(OS2),
        CannedResponse::ok(
            r#"{"policies":[{"_id":"a","policy":{"states":[]}}],"total_policies":1}"#,
        ),
    ])
    .await;
    let out =
        std::env::temp_dir().join(format!("edrs-contract-{}-ism-out.json", std::process::id()));
    let o = meta_run(&[
        "--input",
        &host(&mock),
        "--output",
        out.to_str().unwrap(),
        "--type",
        "policy",
        "--overwrite",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[1].starts_with("GET /_plugins/_ism/policies?size=1000&from=0 "),
        "{}",
        mock.requests()[1]
    );
    let line = std::fs::read_to_string(&out).unwrap();
    assert!(line.starts_with("\"{\\\"policies\\\""), "{line}");
}

#[tokio::test]
async fn cross_engine_policy_restores_are_refused_without_the_flag() {
    let ilm = write_tmp(
        "ilm.json",
        r#"{"p":{"policy":{"phases":{"delete":{"min_age":"1d","actions":{"delete":{}}}}}}}"#,
    );
    let mock = MockEs::serve(vec![CannedResponse::ok(OS2)]).await;
    let o = meta_run(&[
        "--input",
        ilm.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "policy",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--convertIlmToIsm"), "{}", stderr(&o));
    assert_eq!(mock.request_count(), 1);

    // with the flag: translated and PUT as ISM
    let mock = MockEs::serve(vec![
        CannedResponse::ok(OS2),
        CannedResponse::ok(r#"{"_id":"p"}"#),
    ])
    .await;
    let o = meta_run(&[
        "--input",
        ilm.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "policy",
        "--convertIlmToIsm",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let put = &mock.requests()[1];
    assert!(put.starts_with("PUT /_plugins/_ism/policies/p "), "{put}");
    assert!(
        put.contains(r#""states":[{"name":"pending""#) && put.contains(r#""delete":{}"#),
        "{put}"
    );

    let ism = write_tmp(
        "ism2.json",
        r#"{"policies":[{"_id":"a","policy":{"states":[]}}]}"#,
    );
    let mock = MockEs::serve(vec![CannedResponse::ok(ES8)]).await;
    let o = meta_run(&[
        "--input",
        ism.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "policy",
        "--convertIlmToIsm",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("ISM → ILM conversion is not supported"),
        "{}",
        stderr(&o)
    );
}

#[tokio::test]
async fn template_skip_existing_uses_create_and_index_template_needs_components() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::new(400, r#"{"error":{"type":"illegal_argument_exception","reason":"index template [t] already exists"},"status":400}"#),
    ])
    .await;
    let f = write_tmp(
        "tpl.json",
        r#""{\"t\":{\"order\":0,\"index_patterns\":[\"te*\"],\"settings\":{},\"mappings\":{\"type1\":{\"_source\":{\"enabled\":false}}},\"aliases\":{}}}""#,
    );
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "template",
        "--skip-existing",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let put = &mock.requests()[1];
    assert!(put.starts_with("PUT /_template/t?create=true "), "{put}");
    assert!(
        put.contains(r#""mappings":{"_source":{"enabled":false}}"#),
        "{put}"
    );

    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::new(
            404,
            r#"{"error":{"type":"resource_not_found_exception"},"status":404}"#,
        ),
    ])
    .await;
    let f = write_tmp(
        "itpl.json",
        r#""{\"index_templates\":[{\"name\":\"it\",\"index_template\":{\"index_patterns\":[\"x*\"],\"composed_of\":[\"c1\"]}}]}""#,
    );
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--type",
        "index_template",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("restore component templates first"),
        "{}",
        stderr(&o)
    );
    assert!(mock.requests()[1].starts_with("GET /_component_template/c1 "));
    assert_eq!(mock.request_count(), 2);

    // wrong --type for the file: refused before the target is contacted
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        "http://127.0.0.1:1",
        "--type",
        "template",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("use --type index_template"),
        "{}",
        stderr(&o)
    );
}

#[tokio::test]
async fn analyzer_close_index_reopens_even_when_the_update_fails() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::ok("{}"), // HEAD t
        CannedResponse::ok(r#"{"acknowledged":true}"#), // _close
        CannedResponse::new(400, r#"{"error":{"type":"illegal_argument_exception","reason":"bad analyzer"},"status":400}"#),
        CannedResponse::ok(r#"{"acknowledged":true}"#), // _open
        CannedResponse::ok(r#"{"status":"yellow"}"#),
    ])
    .await;
    let f = write_tmp(
        "an.json",
        r#""{\"s\":{\"settings\":{\"index\":{\"analysis\":{\"analyzer\":{\"a\":{\"type\":\"custom\",\"tokenizer\":\"nope\"}}}}}}}""#,
    );
    let out = format!("{}/t", host(&mock));
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &out,
        "--type",
        "analyzer",
        "--closeIndex",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    let reqs = mock.requests();
    let lines: Vec<&str> = reqs.iter().map(|r| r.lines().next().unwrap()).collect();
    assert!(lines[1].starts_with("HEAD /t "), "{lines:?}");
    assert!(lines[2].starts_with("POST /t/_close "), "{lines:?}");
    assert!(lines[3].starts_with("PUT /t/_settings "), "{lines:?}");
    assert!(lines[4].starts_with("POST /t/_open "), "{lines:?}");
    assert!(stderr(&o).contains("was reopened"), "{}", stderr(&o));

    // without --closeIndex an existing index is an error naming both flags
    let mock = MockEs::serve(vec![CannedResponse::ok(ES8), CannedResponse::ok("{}")]).await;
    let out = format!("{}/t", host(&mock));
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &out,
        "--type",
        "analyzer",
    ])
    .await;
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("--closeIndex") && stderr(&o).contains("--skip-existing"),
        "{}",
        stderr(&o)
    );
}

#[tokio::test]
async fn script_dump_reads_cluster_state() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::ok(
            r#"{"metadata":{"stored_scripts":{"s1":{"lang":"mustache","source":"{}"}}}}"#,
        ),
    ])
    .await;
    let o = meta_run(&[
        "--input",
        &host(&mock),
        "--output",
        "$",
        "--type",
        "script",
        "--quiet",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[1]
            .starts_with("GET /_cluster/state/metadata?filter_path=**.stored_scripts.* "),
        "{}",
        mock.requests()[1]
    );
    assert_eq!(
        stdout(&o).trim(),
        r#""{\"stored_scripts\":[{\"_id\":\"s1\",\"found\":true,\"script\":{\"lang\":\"mustache\",\"source\":\"{}\"}}]}""#
    );
}

#[tokio::test]
async fn host_only_index_and_alias_dumps_use_wildcards_and_node_alias_url() {
    let mock = MockEs::serve(vec![CannedResponse::ok(ES8), CannedResponse::ok("{}")]).await;
    let o = meta_run(&[
        "--input",
        &host(&mock),
        "--output",
        "$",
        "--type",
        "index",
        "--quiet",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[1].starts_with("GET /* "),
        "{}",
        mock.requests()[1]
    );

    let mock = MockEs::serve(vec![CannedResponse::ok(ES8), CannedResponse::ok("{}")]).await;
    let url = format!("{}/idx/al", host(&mock));
    let o = meta_run(&["--input", &url, "--output", "$", "--type", "alias"]).await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(
        mock.requests()[1].starts_with("GET /idx/_alias/al "),
        "{}",
        mock.requests()[1]
    );
    assert!(stderr(&o).contains("<index>/<alias>"), "{}", stderr(&o));
}

#[tokio::test]
async fn rename_rules_apply_to_data_and_conflict_with_an_output_index() {
    let o = run(&[
        "--input",
        "in.json",
        "--output",
        "http://127.0.0.1:1/x",
        "--renamePrefix",
        "a-:b-",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(
        stderr(&o).contains("explicit output index"),
        "{}",
        stderr(&o)
    );
    let o = run(&[
        "--input",
        "in.json",
        "--output",
        "http://127.0.0.1:1",
        "--renameIndex",
        "(?=a):b",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("look-around"), "{}", stderr(&o));

    let mock = MockEs::serve(vec![
        CannedResponse::ok(ES8),
        CannedResponse::ok(
            r#"{"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}}]}"#,
        ),
        CannedResponse::ok("{}"),
    ])
    .await;
    let f = write_tmp(
        "data.json",
        "{\"_index\":\"logstash-2026-01-01\",\"_id\":\"1\",\"_source\":{}}\n{\"_index\":\"other\",\"_id\":\"2\",\"_source\":{}}\n",
    );
    let o = meta_run(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &host(&mock),
        "--renamePrefix",
        "logstash-:logstash-srctag-",
        "--suffix",
        "_r",
    ])
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let bulk = &mock.requests()[1];
    assert!(
        bulk.contains(r#""_index":"logstash-srctag-2026-01-01_r""#),
        "{bulk}"
    );
    assert!(bulk.contains(r#""_index":"other_r""#), "{bulk}");
}

// L-405 (TST-API-010): the scroll id is rotated from every page, and a clear that returns
// 404 (contexts already expired) is not an error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scroll_id_rotates_and_clear_404_after_expiry_is_ok() {
    let mock = MockEs::serve(vec![
        CannedResponse::ok(page("s1", &[hit("1", "{\"a\":1}")], 2)),
        CannedResponse::ok(page("s2", &[hit("2", "{\"a\":2}")], 2)),
        CannedResponse::ok(page("s3", &[], 2)),
        CannedResponse::new(404, r#"{"succeeded":true,"num_freed":0}"#),
    ])
    .await;
    let o = run_async(
        vec![
            "--input".into(),
            mock.input_url("idx"),
            "--output".into(),
            "$".into(),
            "--quiet".into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 2, "{}", stdout(&o));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 4, "{reqs:#?}");
    assert!(
        reqs[1].contains("\"s1\""),
        "continuation 1 uses s1: {}",
        reqs[1]
    );
    assert!(
        reqs[2].contains("\"s2\""),
        "continuation 2 uses the rotated s2: {}",
        reqs[2]
    );
    assert!(
        reqs[3].starts_with("DELETE") && reqs[3].contains("s3"),
        "clear the latest id: {}",
        reqs[3]
    );
}

// L-408 (TST-API-013): `hits.total` variants. An int total (`rest_total_hits_as_int`, ES 6
// style) and `{relation: eq}` are exact and reconciled; `gte`, a missing total and a user
// `track_total_hits: false` are estimates and never fail the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hits_total_variants_are_reconciled_only_when_exact() {
    let sa_hits = r#"[{"_index":"idx","_id":"1","_source":{"a":1},"sort":[1]},{"_index":"idx","_id":"2","_source":{"a":2},"sort":[2]}]"#;
    let cases: [(&str, &str, Option<i32>, &str); 6] = [
        ("int exact", r#""total":2,"#, Some(0), ""),
        ("int short", r#""total":5,"#, Some(1), "retrieved 2 of 5"),
        (
            "eq short",
            r#""total":{"value":3,"relation":"eq"},"#,
            Some(1),
            "retrieved 2 of 3",
        ),
        (
            "gte",
            r#""total":{"value":10000,"relation":"gte"},"#,
            Some(0),
            "",
        ),
        ("missing", "", Some(0), ""),
        ("track_total_hits false", "", Some(0), ""),
    ];
    for (name, total, code, msg) in cases {
        let body = format!(
            r#"{{"took":1,"timed_out":false,"_shards":{{"total":1,"successful":1,"skipped":0,"failed":0}},"hits":{{{total}"hits":{sa_hits}}}}}"#
        );
        let mock = MockEs::serve(vec![CannedResponse::ok(body)]).await;
        let search_body = if name.starts_with("track") {
            r#"{"query":{"match_all":{}},"sort":[{"a":"asc"}],"track_total_hits":false}"#
        } else {
            r#"{"query":{"match_all":{}},"sort":[{"a":"asc"}]}"#
        };
        let o = run_async(
            vec![
                "--input".into(),
                mock.input_url("idx"),
                "--output".into(),
                "$".into(),
                "--quiet".into(),
                "--searchAfter".into(),
                "--searchBody".into(),
                search_body.into(),
            ],
            vec![],
        )
        .await;
        assert_eq!(o.status.code(), code, "{name}: {}", stderr(&o));
        assert!(stderr(&o).contains(msg), "{name}: {}", stderr(&o));
        if name.starts_with("track") {
            assert!(
                mock.requests()[0].contains("\"track_total_hits\":false"),
                "user flag kept: {}",
                mock.requests()[0]
            );
        }
    }
}

// L-412 (TST-API-021): bulk responses that the live engines cannot be made to produce on
// demand: per-item 429 (`es_rejected_execution_exception`) is retried for those items only;
// a 413 for the whole request and an item-count mismatch fail the run (exit 1) naming the cause.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_item_429_retry_413_and_item_count_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        format!("{}\n{}\n", hit("1", "{\"a\":1}"), hit("2", "{\"a\":2}")),
    )
    .unwrap();
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let refresh = r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#;
    let args = |url: String| -> Vec<String> {
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            url,
            "--noRefresh".into(),
        ]
    };
    // per-item 429, then success for the retried item only
    let mock = MockEs::serve(vec![
        CannedResponse::ok(info),
        CannedResponse::ok(r#"{"took":1,"errors":true,"items":[{"index":{"status":201}},{"index":{"status":429,"error":{"type":"es_rejected_execution_exception","reason":"rejected execution"}}}]}"#),
        CannedResponse::ok(r#"{"took":1,"errors":false,"items":[{"index":{"status":201}}]}"#),
        CannedResponse::ok(refresh),
    ])
    .await;
    let o = run_async(args(mock.input_url("dst")), vec![]).await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("Total Writes: 2"), "{}", stderr(&o));
    let reqs = mock.requests();
    assert_eq!(reqs.len(), 3, "{reqs:#?}");
    assert!(
        reqs[2].contains("\"_id\":\"2\"") && !reqs[2].contains("\"_id\":\"1\""),
        "only the rejected item is resent: {}",
        reqs[2]
    );
    // 413 for the whole request
    let mock = MockEs::serve(vec![
        CannedResponse::ok(info),
        CannedResponse::new(413, r#"{"error":"Request Entity Too Large"}"#),
    ])
    .await;
    let o = run_async(args(mock.input_url("dst")), vec![]).await;
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("413"), "{}", stderr(&o));
    // fewer items than documents
    let mock = MockEs::serve(vec![
        CannedResponse::ok(info),
        CannedResponse::ok(r#"{"took":1,"errors":true,"items":[{"index":{"status":201}}]}"#),
    ])
    .await;
    let o = run_async(args(mock.input_url("dst")), vec![]).await;
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("has 1 item(s) for 2 document(s)"),
        "{}",
        stderr(&o)
    );
}

// L-413 (TST-API-022): one `_refresh` per written batch (concurrency 1), none with
// `--noRefresh`; a failing refresh is a warning, not a failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refresh_per_batch_no_refresh_and_refresh_failure_is_a_warning() {
    use support::router_mock::{Reply, Route, RouterMock};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        format!(
            "{}\n{}\n{}\n",
            hit("1", r#"{"n":1}"#),
            hit("2", r#"{"n":1}"#),
            hit("3", r#"{"n":1}"#)
        ),
    )
    .unwrap();
    for (extra, refresh_status, want_refreshes) in [
        (None, 200, 3),
        (Some("--noRefresh"), 200, 0),
        (None, 500, 3),
    ] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(
                r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#,
            )))
            .route(Route::contains("POST", "/_bulk").reply(Reply::ok(
                r#"{"took":1,"errors":false,"items":[{"index":{"status":201}}]}"#,
            )))
            .route(
                Route::contains("POST", "/_refresh")
                    .reply(Reply::json(refresh_status, r#"{"error":"boom"}"#)),
            )
            .start()
            .await;
        let mut a: Vec<String> = vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            format!("{}/dst", m.url()),
            "--limit".into(),
            "1".into(),
            "--retryAttempts".into(),
            "0".into(),
        ];
        a.extend(extra.map(String::from));
        let o = run_async(a, vec![]).await;
        assert!(
            o.status.success(),
            "{extra:?}/{refresh_status}: {}",
            stderr(&o)
        );
        let reqs = m.requests();
        let bulks = reqs.iter().filter(|r| r.path.ends_with("/_bulk")).count();
        let refreshes = reqs
            .iter()
            .filter(|r| r.path.ends_with("/_refresh"))
            .count();
        assert_eq!(
            (bulks, refreshes),
            (3, want_refreshes),
            "{extra:?}/{refresh_status}"
        );
        if refresh_status != 200 {
            assert!(
                stderr(&o).contains("refresh failed: HTTP 500"),
                "{}",
                stderr(&o)
            );
        }
    }
}

// L-419/L-425 (TST-API-035, TST-API-041): deprecation `Warning` headers are tolerated and each
// distinct text is logged once; `X-Elastic-Product` is ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warning_headers_are_logged_once_and_not_fatal() {
    use support::router_mock::{Reply, Route, RouterMock};
    let w = r#"299 Elasticsearch-8.19.23-abc "this request accesses system indices, but in a future major version direct access will be prevented by default""#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(
            Reply::ok(r#"{"version":{"number":"8.19.23","build_flavor":"default"},"tagline":"You Know, for Search"}"#)
                .with_header("X-Elastic-Product", "Elasticsearch"),
        ))
        .route(Route::contains("POST", "/idx/_search").reply(Reply::ok(page("s1", &[hit("1", r#"{"n":1}"#)], 2)).with_header("Warning", w)))
        .route(Route::exact("POST", "/_search/scroll").replies([
            Reply::ok(page("s1", &[hit("2", r#"{"n":1}"#)], 2)).with_header("Warning", w),
            Reply::ok(page("s1", &[], 2)).with_header("Warning", w),
        ]))
        .route(Route::prefix("DELETE", "/_search/scroll").reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)))
        .start()
        .await;
    let o = run_async(
        vec![
            "--input".into(),
            format!("{}/idx", m.url()),
            "--output".into(),
            "$".into(),
            "--limit".into(),
            "1".into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o).lines().count(), 2);
    assert_eq!(
        stderr(&o)
            .matches("server warning: this request accesses system indices")
            .count(),
        1,
        "{}",
        stderr(&o)
    );
}

// L-421 (TST-API-037): an ISM overwrite that 409s again (changed concurrently) is reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ism_second_409_is_reported() {
    use support::router_mock::{Reply, Route, RouterMock};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pol.json");
    std::fs::write(&input, r#"{"policies":[{"_id":"p1","_seq_no":3,"_primary_term":1,"policy":{"policy_id":"p1","default_state":"hot","states":[{"name":"hot","actions":[],"transitions":[]}]}}],"total_policies":1}"#).unwrap();
    let conflict = r#"{"error":{"type":"version_conflict_engine_exception","reason":"conflict"},"status":409}"#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(
            r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#,
        )))
        .route(
            Route::exact("GET", "/_plugins/_ism/policies/p1").reply(Reply::ok(
                r#"{"_id":"p1","_seq_no":7,"_primary_term":2,"policy":{}}"#,
            )),
        )
        .route(Route::exact("PUT", "/_plugins/_ism/policies/p1").reply(Reply::json(409, conflict)))
        .start()
        .await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            m.url(),
            "--type".into(),
            "policy".into(),
            "--retryAttempts".into(),
            "0".into(),
        ],
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(
        stderr(&o).contains("changed concurrently"),
        "{}",
        stderr(&o)
    );
    let puts: Vec<_> = m
        .requests()
        .into_iter()
        .filter(|r| r.method == "PUT")
        .collect();
    assert_eq!(puts.len(), 2);
    assert!(
        puts[1]
            .query
            .iter()
            .any(|(k, v)| k == "if_seq_no" && v == "7"),
        "{:?}",
        puts[1].query
    );
    assert!(
        puts[1]
            .query
            .iter()
            .any(|(k, v)| k == "if_primary_term" && v == "2")
    );
}

// L-423 (TST-API-039): a 403 on the version check or on `_cluster/state/metadata` names the
// 'monitor' cluster privilege and never echoes the password.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn script_403_names_the_monitor_privilege() {
    use support::router_mock::{Reply, Route, RouterMock};
    let denied = r#"{"error":{"type":"security_exception","reason":"action [cluster:monitor/main] is unauthorized"},"status":403}"#;
    for root_denied in [true, false] {
        let mut b = RouterMock::builder();
        b = if root_denied {
            b.route(Route::exact("GET", "/").reply(Reply::json(403, denied)))
        } else {
            b.route(Route::exact("GET", "/").reply(Reply::ok(
                r#"{"version":{"number":"8.19.0","build_flavor":"default"},"tagline":"You Know, for Search"}"#,
            )))
            .route(Route::exact("GET", "/_cluster/state/metadata").reply(Reply::json(403, denied)))
        };
        let m = b.start().await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("s.json");
        let url = m.url().replacen("http://", "http://low:s3cr3t-pw@", 1);
        let o = run_async(
            vec![
                "--input".into(),
                url,
                "--output".into(),
                out.to_str().unwrap().into(),
                "--type".into(),
                "script".into(),
                "--retryAttempts".into(),
                "0".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains("403") && err.contains("'monitor' cluster privilege"),
            "{err}"
        );
        assert!(!err.contains("s3cr3t-pw"), "{err}");
        assert!(!out.exists());
    }
}

// L-424 (TST-API-040): `GET /` variants. A proxy/login page or a version-less body fails
// with a clear message (never a silent "Elasticsearch 5.0.0"); OpenSearch 1.x with
// `compatibility.override_main_response_version` is detected by its tagline.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cluster_info_variants_never_fall_back_silently() {
    use support::router_mock::{Reply, Route, RouterMock};
    for (body, needle) in [
        (
            "<html><title>Proxy login</title></html>",
            "did not return JSON",
        ),
        (
            r#"{"name":"n","cluster_name":"c"}"#,
            "without version.number",
        ),
    ] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(body)))
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("o.json");
        let o = run_async(
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                out.to_str().unwrap().into(),
                "--type".into(),
                "mapping".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains(needle) && err.contains("Failed to detect the engine"),
            "{err}"
        );
        assert!(!err.contains("5.0.0"), "{err}");
        assert!(!out.exists());
    }
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(
            r#"{"version":{"number":"7.10.2","build_flavor":"oss"},"tagline":"The OpenSearch Project: https://opensearch.org/"}"#,
        )))
        .route(Route::exact("GET", "/_plugins/_ism/policies").reply(Reply::ok(
            r#"{"policies":[],"total_policies":0}"#,
        )))
        .start()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("p.json");
    let o = run_async(
        vec![
            "--input".into(),
            m.url(),
            "--output".into(),
            out.to_str().unwrap().into(),
            "--type".into(),
            "policy".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("compatibility.override_main_response_version"),
        "{err}"
    );
    assert!(
        m.requests()
            .iter()
            .any(|r| r.path.starts_with("/_plugins/_ism/policies"))
    );
    assert!(!m.requests().iter().any(|r| r.path.starts_with("/_ilm")));
}

// L-654 (BRK-XV-001): OpenSearch with `compatibility.override_main_response_version: true`
// answers `GET /` as Elasticsearch 7.10.2 (the live 1.3.20 body: no distribution, no
// build_flavor). The real version comes from `GET _nodes`; when a proxy also rewrote the
// tagline and `_nodes` is refused, the run fails clearly instead of guessing Elasticsearch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn override_main_response_version_is_detected_through_the_nodes() {
    use support::router_mock::{Reply, Route, RouterMock};
    const ROOT: &str = r#"{"name":"n","cluster_name":"c","version":{"number":"7.10.2","build_type":"tar","build_hash":"31afd17a1b5a22338307e1f4a78092887c1490e3","lucene_version":"8.10.1","minimum_wire_compatibility_version":"6.8.0"},"tagline":"The OpenSearch Project: https://opensearch.org/"}"#;
    let args = |url: String, out: &std::path::Path| {
        vec![
            "--input".into(),
            url,
            "--output".into(),
            out.to_str().unwrap().into(),
            "--type".into(),
            "policy".into(),
        ]
    };
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(ROOT)))
        .route(Route::exact("GET", "/_nodes").reply(Reply::ok(
            r#"{"nodes":{"kOmQoGFZTIWNKMcDlyF5wg":{"version":"1.3.20"}}}"#,
        )))
        .route(
            Route::exact("GET", "/_plugins/_ism/policies")
                .reply(Reply::ok(r#"{"policies":[],"total_policies":0}"#)),
        )
        .start()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("p.json");
    let o = run_async(args(m.url(), &out), vec![]).await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("OpenSearch 1.3.20 reports version 7.10.2 on GET /")
            && err.contains("using 1.3.20 from GET _nodes"),
        "{err}"
    );
    let reqs = m.requests();
    assert!(
        reqs.iter()
            .any(|r| r.path == "/_nodes" && r.query.iter().any(|(k, _)| k == "filter_path")),
        "{reqs:?}"
    );
    assert!(
        reqs.iter()
            .any(|r| r.path.starts_with("/_plugins/_ism/policies"))
    );
    assert!(
        !reqs.iter().any(|r| r.path.starts_with("/_ilm")),
        "{reqs:?}"
    );

    // A proxy rewrote the tagline too and `_nodes` is refused: nothing says which engine.
    let rewritten = ROOT.replace(
        "The OpenSearch Project: https://opensearch.org/",
        "You Know, for Search",
    );
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(rewritten)))
        .route(Route::exact("GET", "/_nodes").reply(Reply::json(403, r#"{"error":"forbidden"}"#)))
        .start()
        .await;
    let out = dir.path().join("q.json");
    let o = run_async(args(m.url(), &out), vec![]).await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("Failed to detect the engine")
            && err.contains("compatibility.override_main_response_version")
            && err.contains("403"),
        "{err}"
    );
    assert!(!out.exists());
    assert!(
        !m.requests()
            .iter()
            .any(|r| r.path.starts_with("/_ilm") || r.path.starts_with("/_plugins")),
        "{:?}",
        m.requests()
    );
}

// L-655 (BRK-XV-004): a mapping with field types the target has no type for is refused
// before anything is written, naming every field, its type and the closest type; with
// --ignore-errors only that index is skipped. A cross-engine cluster-to-cluster data copy
// into a missing index (dynamic mappings would guess the types) is refused the same way.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fields_without_an_equivalent_are_refused_before_any_write() {
    use support::router_mock::{Reply, Route, RouterMock};
    const OS2: &str = r#"{"version":{"distribution":"opensearch","number":"2.19.7"},"tagline":"The OpenSearch Project: https://opensearch.org/"}"#;
    const ES8: &str = r#"{"version":{"number":"8.19.23","build_flavor":"default"},"tagline":"You Know, for Search"}"#;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("m.json");
    std::fs::write(
        &file,
        r#"{"bad":{"mappings":{"properties":{"k":{"type":"keyword"},"labels":{"type":"flattened"},"obj":{"properties":{"emb":{"type":"dense_vector","dims":3}}}}}},"good":{"mappings":{"properties":{"k":{"type":"keyword"}}}}}"#,
    )
    .unwrap();
    let os_mock = || {
        RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(OS2)))
            .route(Route::prefix("PUT", "/").reply(Reply::ok(r#"{"acknowledged":true}"#)))
            .start()
    };
    let restore = |url: String, ty: &str, extra: &[&str]| {
        let mut a: Vec<String> = vec![
            "--input".into(),
            file.to_str().unwrap().into(),
            "--output".into(),
            url,
            "--type".into(),
            ty.into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    for ty in ["mapping", "index"] {
        let m = os_mock().await;
        let o = run_async(restore(m.url(), ty, &[]), vec![]).await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{err}");
        assert!(
            err.contains(
                "the mapping of 'bad' has 2 field type(s) that OpenSearch 2.19.7 has no type for"
            ) && err.contains("`labels` (flattened: an Elasticsearch-only type")
                && err.contains("flat_object")
                && err.contains("`obj.emb` (dense_vector")
                && err.contains("knn_vector")
                && err.contains("nothing was written"),
            "{ty}: {err}"
        );
        assert!(
            !m.requests().iter().any(|r| r.method == "PUT"),
            "{ty}: {:?}",
            m.requests()
        );
        // --ignore-errors: the other index is still restored
        let m = os_mock().await;
        let o = run_async(restore(m.url(), ty, &["--ignore-errors"]), vec![]).await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(0), "{ty}: {err}");
        assert!(err.contains("`labels` (flattened"), "{err}");
        let puts: Vec<String> = m
            .requests()
            .iter()
            .filter(|r| r.method == "PUT")
            .map(|r| r.path.clone())
            .collect();
        assert!(
            puts.iter().all(|p| p.starts_with("/good")),
            "{ty}: {puts:?}"
        );
        assert!(!puts.is_empty(), "{ty}");
    }

    // cluster to cluster, Elasticsearch -> OpenSearch, target index missing
    let src = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(ES8)))
        .route(Route::exact("GET", "/logs/_mapping").reply(Reply::ok(
            r#"{"logs":{"mappings":{"properties":{"msg":{"type":"text"},"labels":{"type":"flattened"}}}}}"#,
        )))
        .route(Route::any().reply(Reply::json(500, r#"{"error":"no search in this test"}"#)))
        .start()
        .await;
    let dst = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(OS2)))
        .start()
        .await;
    let o = run_async(
        vec![
            "--input".into(),
            format!("{}/logs", src.url()),
            "--output".into(),
            format!("{}/logs", dst.url()),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("the target index 'logs' does not exist")
            && err.contains("dynamic mappings")
            && err.contains("`labels` (flattened")
            && err.contains("nothing was written"),
        "{err}"
    );
    assert!(
        !dst.requests().iter().any(|r| r.path.contains("_bulk")),
        "{:?}",
        dst.requests()
    );
    assert!(
        !src.requests().iter().any(|r| r.path.contains("_search")),
        "{:?}",
        src.requests()
    );
}

// L-656 (BRK-XV-005): features the target does not have fail clearly before any write: an
// Elasticsearch OSS build (no X-Pack) refuses templates with a data_stream and ILM settings,
// and settings restores with ILM keys; OpenSearch refuses ILM settings. Credentials sent to an
// OSS build are warned about (it does not check them), and a data stream written with
// `--bulkAction index` names `--bulkAction create` once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_features_on_the_target_fail_clearly_before_any_write() {
    use support::router_mock::{Reply, Route, RouterMock};
    const OSS: &str =
        r#"{"version":{"number":"7.10.2","build_flavor":"oss"},"tagline":"You Know, for Search"}"#;
    const OS13: &str = r#"{"version":{"distribution":"opensearch","number":"1.3.20"},"tagline":"The OpenSearch Project: https://opensearch.org/"}"#;
    let dir = tempfile::tempdir().unwrap();
    let tpl = dir.path().join("t.json");
    std::fs::write(
        &tpl,
        r#"{"ds":{"index_patterns":["ds-*"],"data_stream":{},"template":{"settings":{"number_of_replicas":0}}},"ilm":{"index_patterns":["lc-*"],"template":{"settings":{"index":{"lifecycle":{"name":"p1"}},"frozen":true}}},"good":{"index_patterns":["g-*"],"template":{"settings":{"number_of_replicas":0}}}}"#,
    )
    .unwrap();
    let settings = dir.path().join("s.json");
    std::fs::write(
        &settings,
        r#"{"lc":{"settings":{"index.lifecycle.name":"p1","index":{"number_of_replicas":"0","routing":{"allocation":{"include":{"_tier_preference":"data_hot"}}}}}}}"#,
    )
    .unwrap();
    let mock = |info: &'static str| {
        RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::prefix("PUT", "/").reply(Reply::ok(r#"{"acknowledged":true}"#)))
            .start()
    };
    let args = |input: &std::path::Path, url: String, ty: &str, extra: &[&str]| {
        let mut a: Vec<String> = vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            url,
            "--type".into(),
            ty.into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let puts = |m: &RouterMock| -> Vec<String> {
        m.requests()
            .iter()
            .filter(|r| r.method == "PUT")
            .map(|r| r.path.clone())
            .collect()
    };

    // templates on the OSS build
    let m = mock(OSS).await;
    let o = run_async(args(&tpl, m.url(), "index_template", &[]), vec![]).await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("Elasticsearch 7.10.2 (OSS build) cannot take these")
            && err.contains(
                "index_template 'ds' has a data_stream (data streams are an X-Pack feature"
            )
            && err.contains("--excludeTemplateFields=data_stream")
            && err.contains("index_template 'ilm' has settings index.frozen, index.lifecycle.name")
            && err.contains("--excludeSettings=index.frozen,index.lifecycle.name")
            && !err.contains("'good'")
            && err.contains("nothing was written"),
        "{err}"
    );
    assert!(puts(&m).is_empty(), "{:?}", m.requests());
    let m = mock(OSS).await;
    let o = run_async(
        args(&tpl, m.url(), "index_template", &["--ignore-errors"]),
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(puts(&m), vec!["/_index_template/good".to_string()]);
    // the exclusions the hint names are enough
    let m = mock(OSS).await;
    let o = run_async(
        args(
            &tpl,
            m.url(),
            "index_template",
            &[
                "--excludeTemplateFields=data_stream",
                "--excludeSettings=index.lifecycle.name,index.frozen",
            ],
        ),
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(puts(&m).len(), 3, "{:?}", m.requests());

    // settings: ILM refused on the OSS build and on OpenSearch; _tier_preference is not
    for (info, who) in [
        (OSS, "Elasticsearch 7.10.2 (OSS build)"),
        (OS13, "OpenSearch 1.3.20"),
    ] {
        for ty in ["settings", "index"] {
            let m = mock(info).await;
            let o = run_async(args(&settings, m.url(), ty, &[]), vec![]).await;
            let err = stderr(&o);
            assert_eq!(o.status.code(), Some(1), "{ty}: {err}");
            assert!(
                err.contains(&format!(
                    "the settings of 'lc' have index.lifecycle.name that {who} rejects"
                )) && err.contains("--excludeSettings=index.lifecycle.name;")
                    && !err.contains("_tier_preference that")
                    && err.contains("nothing was written"),
                "{ty}: {err}"
            );
            assert!(puts(&m).is_empty(), "{ty}: {:?}", m.requests());
        }
    }

    // credentials sent to the OSS build
    let data = dir.path().join("d.json");
    std::fs::write(
        &data,
        r#"{"_index":"x","_id":"1","_source":{"a":1}}"#.to_owned() + "\n",
    )
    .unwrap();
    for (info, warned) in [(OSS, true), (OS13, false)] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::contains("POST", "_bulk").reply(Reply::ok(
                r#"{"took":1,"errors":false,"items":[{"index":{"_index":"x","_id":"1","status":201}}]}"#,
            )))
            .route(Route::any().reply(Reply::ok("{}")))
            .start()
            .await;
        let url = m.url().replace("http://", "http://u:FAKE-pw@") + "/x";
        let o = run_async(args(&data, url, "data", &[]), vec![]).await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(0), "{err}");
        assert_eq!(
            err.contains(
                "(OSS build), which has no X-Pack security: it does not check the credentials"
            ),
            warned,
            "{err}"
        );
        assert!(!err.contains("FAKE-pw"), "{err}");
    }

    // a data stream written with `index`: the fix is named once
    std::fs::write(
        &data,
        r#"{"_index":"ds-b","_id":"1","_source":{"@timestamp":"2026-01-01T00:00:00Z"}}
{"_index":"ds-b","_id":"2","_source":{"@timestamp":"2026-01-01T00:00:00Z"}}
"#,
    )
    .unwrap();
    let item = r#"{"index":{"_index":"ds-b","_id":"1","status":400,"error":{"type":"illegal_argument_exception","reason":"only write ops with an op_type of create are allowed in data streams"}}}"#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(OS13)))
        .route(Route::contains("POST", "_bulk").reply(Reply::ok(format!(
            r#"{{"took":1,"errors":true,"items":[{item},{item}]}}"#
        ))))
        .route(Route::any().reply(Reply::ok("{}")))
        .start()
        .await;
    let o = run_async(
        args(&data, format!("{}/ds-b", m.url()), "data", &[]),
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert_eq!(
        err.matches("the target is a data stream, which accepts only `create`")
            .count(),
        1,
        "{err}"
    );
    assert!(err.contains("re-run with --bulkAction create"), "{err}");
}

// L-425 (TST-API-041): `X-Elastic-Product` missing or unexpected is tolerated (the tool does not
// enforce the official client's product check); a `Warning` on `GET /` is logged once.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn x_elastic_product_is_tolerated_on_meta_paths() {
    use support::router_mock::{Reply, Route, RouterMock};
    for product in [None, Some("Elasticsearch"), Some("SomethingElse")] {
        let w = r#"299 Elasticsearch-8.19.23-abc "[ignore_throttled] parameter is deprecated""#;
        let mut root = Reply::ok(
            r#"{"version":{"number":"8.19.23","build_flavor":"default"},"tagline":"You Know, for Search"}"#,
        )
        .with_header("Warning", w);
        let mut mapping =
            Reply::ok(r#"{"idx":{"mappings":{"properties":{"a":{"type":"keyword"}}}}}"#)
                .with_header("Warning", w);
        if let Some(p) = product {
            root = root.with_header("X-Elastic-Product", p);
            mapping = mapping.with_header("X-Elastic-Product", p);
        }
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(root))
            .route(Route::exact("GET", "/idx/_mapping").reply(mapping))
            .start()
            .await;
        let o = run_async(
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                "$".into(),
                "--type".into(),
                "mapping".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert!(o.status.success(), "{product:?}: {err}");
        assert!(stdout(&o).contains("keyword"), "{}", stdout(&o));
        assert_eq!(
            err.matches("server warning: [ignore_throttled] parameter is deprecated")
                .count(),
            1,
            "{err}"
        );
    }
}

// L-427 (TST-API-051): 5xx per request kind. Idempotent requests (initial search, meta GET)
// are retried; a scroll continuation is not (the cursor may have moved) and the message says
// why and how to recover. Exit 1, nothing committed, no staging left behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn http_5xx_per_request_kind() {
    use support::router_mock::{Reply, Route, RouterMock};
    for status in [500u16, 502, 503, 504] {
        let fail =
            format!(r#"{{"error":{{"type":"x","reason":"boom {status}"}},"status":{status}}}"#);
        let m = RouterMock::builder()
            .route(Route::contains("POST", "/idx/_search").replies([
                Reply::json(status, &fail),
                Reply::ok(page("s1", &[hit("1", r#"{"n":1}"#)], 3)),
            ]))
            .route(Route::exact("POST", "/_search/scroll").reply(Reply::json(status, &fail)))
            .route(
                Route::prefix("DELETE", "/_search/scroll")
                    .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
            )
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("d.json");
        let o = run_async(
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                out.to_str().unwrap().into(),
                "--limit".into(),
                "1".into(),
                "--retryAttempts".into(),
                "2".into(),
                "--retryDelay".into(),
                "1".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(4), "{err}");
        let reqs = m.requests();
        let n = |p: &str| {
            reqs.iter()
                .filter(|r| r.method == "POST" && r.path.starts_with(p))
                .count()
        };
        assert_eq!(n("/idx/_search"), 2, "initial search retried once: {err}");
        assert_eq!(
            n("/_search/scroll"),
            1,
            "continuation never retried on {status}: {err}"
        );
        assert!(
            err.contains(&format!("HTTP {status}")) && err.contains(&format!("boom {status}")),
            "{err}"
        );
        assert!(
            err.contains("not retried") && err.contains("--scrollId"),
            "{err}"
        );
        assert!(!out.exists(), "nothing committed");
        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(left.is_empty(), "staging left: {left:?}");
    }
}

// L-428 (TST-API-052): partial results. A failed shard on one slice only, or `timed_out: true`
// on a continuation, fails the dump with the shard reason and commits nothing; skipped shards
// (can_match pre-filtering) are not failures.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_results_fail_and_commit_nothing() {
    use support::router_mock::{Reply, Route, RouterMock};
    let ok = |sid: &str, ids: &[&str]| {
        page(
            sid,
            &ids.iter().map(|i| hit(i, r#"{"n":1}"#)).collect::<Vec<_>>(),
            4,
        )
    };
    let failed_slice1 = r#"{"_scroll_id":"s1","timed_out":false,"_shards":{"total":2,"successful":1,"skipped":0,"failed":1,"failures":[{"shard":1,"index":"idx","node":"n","reason":{"type":"circuit_breaking_exception","reason":"[parent] Data too large"}}]},"hits":{"total":{"value":4,"relation":"eq"},"hits":[]}}"#;
    let timed_out = r#"{"_scroll_id":"s0","timed_out":true,"_shards":{"total":1,"successful":1,"skipped":0,"failed":0},"hits":{"total":{"value":4,"relation":"eq"},"hits":[]}}"#;
    let skipped = r#"{"_scroll_id":"s0","timed_out":false,"_shards":{"total":4,"successful":4,"skipped":3,"failed":0},"hits":{"total":{"value":2,"relation":"eq"},"hits":[{"_index":"idx","_id":"a","_source":{"n":1}},{"_index":"idx","_id":"b","_source":{"n":1}}]}}"#;
    let cases: Vec<(&str, RouterMock, Vec<&str>, Option<&str>)> = vec![
        (
            "one slice fails",
            RouterMock::builder()
                .route(
                    Route::contains("POST", "/idx/_search")
                        .body_contains("\"id\":0")
                        .reply(Reply::ok(ok("s0", &["1", "2"]))),
                )
                .route(
                    Route::contains("POST", "/idx/_search")
                        .body_contains("\"id\":1")
                        .reply(Reply::ok(failed_slice1)),
                )
                .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(ok("s0", &[]))))
                .route(
                    Route::prefix("DELETE", "/_search/scroll")
                        .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
                )
                .start()
                .await,
            vec!["--slices", "2"],
            Some("idx[1]: circuit_breaking_exception: [parent] Data too large"),
        ),
        (
            "continuation timed out",
            RouterMock::builder()
                .route(Route::contains("POST", "/idx/_search").reply(Reply::ok(ok("s0", &["1"]))))
                .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(timed_out)))
                .route(
                    Route::prefix("DELETE", "/_search/scroll")
                        .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
                )
                .start()
                .await,
            vec![],
            Some("timed_out=true"),
        ),
        (
            "skipped shards are fine",
            RouterMock::builder()
                .route(Route::contains("POST", "/idx/_search").reply(Reply::ok(skipped)))
                .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(ok("s0", &[]))))
                .route(
                    Route::prefix("DELETE", "/_search/scroll")
                        .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
                )
                .start()
                .await,
            vec![],
            None,
        ),
    ];
    for (name, m, extra, want) in cases {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("d.json");
        let mut args: Vec<String> = vec![
            "--input".into(),
            format!("{}/idx", m.url()),
            "--output".into(),
            out.to_str().unwrap().into(),
            "--retryAttempts".into(),
            "0".into(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        let o = run_async(args, vec![]).await;
        let err = stderr(&o);
        match want {
            Some(w) => {
                assert_eq!(o.status.code(), Some(1), "{name}: {err}");
                assert!(err.contains(w), "{name}: {err}");
                assert!(!out.exists(), "{name}: committed");
                assert_eq!(
                    std::fs::read_dir(dir.path()).unwrap().count(),
                    0,
                    "{name}: staging left"
                );
            }
            None => {
                assert!(o.status.success(), "{name}: {err}");
                assert_eq!(
                    std::fs::read_to_string(&out).unwrap().lines().count(),
                    2,
                    "{name}"
                );
            }
        }
    }
}

// L-431 (TST-API-055): TLS failures against `openssl s_server` endpoints built from the cert
// factory (scripts/tls/certs.sh). Each failure: exit 1, one message naming the cause, no retry
// loop (TLS errors are permanent). Success cases reach HTTP (s_server -www answers HTML, so the
// run stops at "expected JSON from" (its text/html page)). Skipped when openssl (>= 3.4 for -not_before) is missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_failures_have_one_clear_message() {
    struct Srv(std::process::Child);
    impl Drop for Srv {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let certs = dir.path().join("certs");
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/tls/certs.sh");
    let ok = std::process::Command::new("bash")
        .arg(script)
        .arg(&certs)
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        println!("SKIP: openssl cert factory unavailable");
        return;
    }
    let free_port = || {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let start = |cert: &str, extra: &[&str]| -> (Srv, u16) {
        let port = free_port();
        let child = std::process::Command::new("openssl")
            .current_dir(&certs)
            .args(["s_server", "-quiet", "-www", "-accept", &port.to_string()])
            .args([
                "-cert",
                &format!("{cert}.pem"),
                "-key",
                &format!("{cert}.key"),
            ])
            .args(extra)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        (Srv(child), port)
    };
    let plain_port = free_port();
    let plain = std::net::TcpListener::bind(("127.0.0.1", plain_port)).unwrap();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        for mut s in plain.incoming().flatten() {
            let mut b = [0u8; 1024];
            let _ = s.read(&mut b);
            let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\ncontent-length: 0\r\n\r\n");
        }
    });
    let ca = certs.join("ca.pem").to_str().unwrap().to_string();
    let key = |n: &str| certs.join(n).to_str().unwrap().to_string();
    let servers = [
        ("ok", start("ok", &[])),
        ("expired", start("expired", &[])),
        ("future", start("future", &[])),
        ("wrongname", start("wrongname", &[])),
        ("iponly", start("iponly", &[])),
        ("unknown", start("unknown", &[])),
        ("mtls", start("ok", &["-Verify", "1", "-CAfile", "ca.pem"])),
        (
            "tls11",
            start("ok", &["-tls1_1", "-cipher", "DEFAULT:@SECLEVEL=0"]),
        ),
    ];
    let port = |n: &str| servers.iter().find(|(k, _)| *k == n).unwrap().1.1;
    let reached_http = "expected JSON from";
    type Env = Vec<(String, String)>;
    let cases: Vec<(&str, String, Vec<String>, Env, &str)> = vec![
        (
            "valid",
            format!("https://localhost:{}/idx", port("ok")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            reached_http,
        ),
        (
            "valid by IP SAN",
            format!("https://127.0.0.1:{}/idx", port("iponly")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            reached_http,
        ),
        (
            "unknown CA",
            format!("https://localhost:{}/idx", port("unknown")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "not signed by a trusted CA",
        ),
        (
            "no CA given",
            format!("https://localhost:{}/idx", port("ok")),
            vec![],
            vec![],
            "not signed by a trusted CA",
        ),
        (
            "expired",
            format!("https://localhost:{}/idx", port("expired")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "certificate has expired",
        ),
        (
            "not yet valid",
            format!("https://localhost:{}/idx", port("future")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "not valid yet",
        ),
        (
            "hostname mismatch",
            format!("https://localhost:{}/idx", port("wrongname")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "hostname mismatch",
        ),
        (
            "IP SAN only, by name",
            format!("https://localhost:{}/idx", port("iponly")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "hostname mismatch",
        ),
        (
            "client cert missing",
            format!("https://localhost:{}/idx", port("mtls")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "requires a client certificate",
        ),
        (
            "client cert",
            format!("https://localhost:{}/idx", port("mtls")),
            vec![
                "--input-ca".into(),
                ca.clone(),
                "--input-cert".into(),
                key("client.pem"),
                "--input-key".into(),
                key("client.key"),
            ],
            vec![],
            reached_http,
        ),
        (
            "encrypted key",
            format!("https://localhost:{}/idx", port("mtls")),
            vec![
                "--input-ca".into(),
                ca.clone(),
                "--input-cert".into(),
                key("client.pem"),
                "--input-key".into(),
                key("client-enc.key"),
                "--input-pass".into(),
                "clientpass".into(),
            ],
            vec![],
            reached_http,
        ),
        (
            "wrong --pass",
            format!("https://localhost:{}/idx", port("mtls")),
            vec![
                "--input-ca".into(),
                ca.clone(),
                "--input-cert".into(),
                key("client.pem"),
                "--input-key".into(),
                key("client-enc.key"),
                "--input-pass".into(),
                "WRONG-pass".into(),
            ],
            vec![],
            "wrong --pass",
        ),
        (
            "legacy encrypted PEM",
            format!("https://localhost:{}/idx", port("mtls")),
            vec![
                "--input-ca".into(),
                ca.clone(),
                "--input-cert".into(),
                key("client.pem"),
                "--input-key".into(),
                key("client-legacy.key"),
                "--input-pass".into(),
                "clientpass".into(),
            ],
            vec![],
            "Legacy encrypted PEM key",
        ),
        (
            "TLS 1.1 only",
            format!("https://localhost:{}/idx", port("tls11")),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "TLS 1.2+ is required",
        ),
        (
            "https to plain port",
            format!("https://localhost:{plain_port}/idx"),
            vec!["--input-ca".into(), ca.clone()],
            vec![],
            "use http://",
        ),
        (
            "http to TLS port",
            format!("http://localhost:{}/idx", port("ok")),
            vec![],
            vec![],
            "use https://",
        ),
        (
            "--insecure",
            format!("https://localhost:{}/idx", port("unknown")),
            vec!["--insecure".into()],
            vec![],
            "verification is DISABLED",
        ),
        (
            "NODE_TLS_REJECT_UNAUTHORIZED=0",
            format!("https://localhost:{}/idx", port("unknown")),
            vec![],
            vec![("NODE_TLS_REJECT_UNAUTHORIZED".into(), "0".into())],
            "verification is DISABLED",
        ),
    ];
    for (name, url, extra, env, want) in cases {
        let mut args: Vec<String> = vec![
            "--input".into(),
            url,
            "--output".into(),
            dir.path().join("o.json").to_str().unwrap().into(),
            "--type".into(),
            "mapping".into(),
            "--retryAttempts".into(),
            "2".into(),
            "--retryDelay".into(),
            "1".into(),
        ];
        args.extend(extra);
        let o = run_async(args, env).await;
        let err = stderr(&o);
        // L-804b: a plain-HTTP request whose connection closes is retried and may be transient
        let code = if name == "http to TLS port" { 4 } else { 1 };
        assert_eq!(o.status.code(), Some(code), "{name}: {err}");
        assert!(err.contains(want), "{name}: want {want:?}: {err}");
        assert!(
            !err.contains("clientpass") && !err.contains("WRONG-pass"),
            "{name}: {err}"
        );
        if want != reached_http
            && !name.starts_with("http to")
            && !name.starts_with("--")
            && !name.starts_with("NODE")
        {
            assert!(
                !err.contains("retrying"),
                "{name}: TLS failure was retried: {err}"
            );
        }
        // L-610 (TST-BRK-TLS-001): server certificate problems name the reason, the host
        // and a way out (--ca / --insecure)
        if [
            "unknown CA",
            "no CA given",
            "expired",
            "not yet valid",
            "hostname mismatch",
            "IP SAN only, by name",
        ]
        .contains(&name)
        {
            assert!(
                err.contains("localhost") && err.contains("--insecure"),
                "{name}: host and hint: {err}"
            );
        }
        assert!(!dir.path().join("o.json").exists());
    }
    drop(servers);
}

// L-431a: with TLS 1.3 a server that requires a client certificate completes the handshake,
// then its alert can close the new connection before the request is queued on it; hyper
// reports that as "connection was not ready" (Canceled), without the TLS cause. That was
// classified as a transport failure and retried with a "retrying" warning in about 2% of
// runs (the flake in `tls_failures_have_one_clear_message`). Many runs make the race show:
// every one must name the missing client certificate and never retry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_client_cert_rejection_is_never_retried() {
    struct Srv(std::process::Child);
    impl Drop for Srv {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let certs = dir.path().join("certs");
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/tls/certs.sh");
    let ok = std::process::Command::new("bash")
        .arg(script)
        .arg(&certs)
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        println!("SKIP: openssl cert factory unavailable");
        return;
    }
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let _srv = Srv(std::process::Command::new("openssl")
        .current_dir(&certs)
        .args(["s_server", "-quiet", "-www", "-accept", &port.to_string()])
        .args([
            "-cert", "ok.pem", "-key", "ok.key", "-Verify", "1", "-CAfile", "ca.pem",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap());
    for _ in 0..100 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let ca = certs.join("ca.pem").to_str().unwrap().to_string();
    let out = dir.path().join("o.json").to_str().unwrap().to_string();
    for run in 0..300 {
        let args: Vec<String> = vec![
            "--input".into(),
            format!("https://localhost:{port}/idx"),
            "--output".into(),
            out.clone(),
            "--type".into(),
            "mapping".into(),
            "--retryAttempts".into(),
            "2".into(),
            "--retryDelay".into(),
            "1".into(),
            "--input-ca".into(),
            ca.clone(),
        ];
        let o = run_async(args, vec![]).await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "run {run}: {err}");
        assert!(
            err.contains("requires a client certificate"),
            "run {run}: {err}"
        );
        assert!(!err.contains("retrying"), "run {run}: retried: {err}");
        assert!(!err.contains("not ready"), "run {run}: cause lost: {err}");
    }
}

// L-441 (TST-BRK-NET-003): the connection is reset after the cluster read (and applied) a
// `_bulk` request. Explicit ids: re-sent, harmless. Auto-id documents: never re-sent, the run
// fails and says why (a re-send would duplicate them). `create`: the re-sent request's 409s are
// counted as written by the first attempt, not as failures.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bulk_reset_after_apply_is_never_a_silent_duplicate() {
    use support::router_mock::{Fault, Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let dir = tempfile::tempdir().unwrap();
    let with_ids = dir.path().join("ids.json");
    std::fs::write(
        &with_ids,
        format!("{}\n{}\n", hit("1", "{\"a\":1}"), hit("2", "{\"a\":2}")),
    )
    .unwrap();
    let no_ids = dir.path().join("src.json");
    std::fs::write(&no_ids, "{\"a\":1}\n{\"a\":2}\n").unwrap();
    let mock = |second: &str| {
        let second = second.to_string();
        async move {
            RouterMock::builder()
                .route(Route::exact("GET", "/").reply(Reply::ok(info)))
                .route(
                    Route::contains("POST", "/_bulk")
                        .replies([Reply::ok("{}").with_fault(Fault::Reset), Reply::ok(second)]),
                )
                .fallback(Reply::ok(
                    r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
                ))
                .start()
                .await
        }
    };
    let args = |input: &std::path::Path, url: String, extra: &[&str]| -> Vec<String> {
        let mut a: Vec<String> = vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            url,
            "--noRefresh".into(),
            "--retryDelay".into(),
            "1".into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let bulks = |m: &RouterMock| {
        m.requests()
            .iter()
            .filter(|r| r.path.ends_with("/_bulk"))
            .count()
    };

    // explicit ids: the whole batch is re-sent once and the run succeeds
    let m = mock(
        r#"{"took":1,"errors":false,"items":[{"index":{"status":200}},{"index":{"status":200}}]}"#,
    )
    .await;
    let o = run_async(args(&with_ids, format!("{}/dst", m.url()), &[]), vec![]).await;
    let err = stderr(&o);
    assert!(o.status.success(), "{err}");
    assert_eq!(bulks(&m), 2, "{err}");
    assert!(err.contains("Total Writes: 2"), "{err}");

    // auto ids: sent once, never re-sent, exit 1 with the reason
    let m = mock(
        r#"{"took":1,"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}}]}"#,
    )
    .await;
    let o = run_async(args(&no_ids, format!("{}/dst", m.url()), &[]), vec![]).await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert_eq!(bulks(&m), 1, "an auto-id batch must not be re-sent: {err}");
    assert!(
        err.contains("not re-sent") && err.contains("2 document(s) without _id"),
        "{err}"
    );

    // create: the re-sent request answers 409 for what the first attempt created
    let m = mock(r#"{"took":1,"errors":true,"items":[{"create":{"status":409,"error":{"type":"version_conflict_engine_exception","reason":"document already exists"}}},{"create":{"status":409,"error":{"type":"version_conflict_engine_exception","reason":"document already exists"}}}]}"#).await;
    let o = run_async(
        args(
            &with_ids,
            format!("{}/dst", m.url()),
            &[
                "--bulkAction",
                "create",
                "--ignore-es-write-errors",
                "false",
            ],
        ),
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert!(o.status.success(), "{err}");
    assert_eq!(bulks(&m), 2, "{err}");
    assert!(err.contains("Total Writes: 2"), "{err}");
    assert!(
        err.contains("counted as written by the earlier attempt"),
        "{err}"
    );
}

// L-442 (TST-BRK-NET-004): a server that accepts and never answers, sends headers then stops,
// or stops mid-body cannot hang the run: without --timeout, the idle timeout (--idleTimeout,
// default 300 s) ends the request, it is retried where that is safe, and the run then fails
// with a message naming the limit, well within the configured bound.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stalled_server_fails_within_the_idle_timeout() {
    use std::time::Duration;
    use support::router_mock::{Fault, Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let stall = Duration::from_secs(60);
    let body = page("s1", &[hit("1", r#"{"n":1}"#)], 1);
    for (label, fault) in [
        ("never answers", Fault::Delay(stall)),
        ("headers then silence", Fault::StallAfterHeaders(stall)),
        ("stops mid-body", Fault::StallMidBody { after: 10, stall }),
    ] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(
                Route::contains("POST", "/idx/_search").reply(Reply::ok(&body).with_fault(fault)),
            )
            .fallback(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#))
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("d.json");
        let t = std::time::Instant::now();
        let o = run_async(
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                out.to_str().unwrap().into(),
                "--idleTimeout".into(),
                "1".into(),
                "--retryAttempts".into(),
                "1".into(),
                "--retryDelay".into(),
                "1".into(),
            ],
            vec![],
        )
        .await;
        let took = t.elapsed();
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(4), "{label}: {err}");
        assert!(took < Duration::from_secs(20), "{label}: took {took:?}");
        assert!(err.contains("--idleTimeout"), "{label}: {err}");
        assert!(!out.exists(), "{label}: nothing committed");
        let searches = m
            .requests()
            .iter()
            .filter(|r| r.path.starts_with("/idx/_search"))
            .count();
        assert_eq!(
            searches, 2,
            "{label}: the initial search is retried once: {err}"
        );
    }
}

// L-443 (TST-BRK-NET-006): a load balancer closes idle keep-alive connections (after 300 ms here,
// with a FIN or an RST) while the dump waits longer than that between pages
// (--throttleInterval 600 with a 300 ms idle close; --bufferSize 1 so retrieval waits for the throttled
// sink). The next scroll continuation must not fail on the dead pooled
// connection: the dump completes with every page.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stale_keep_alive_connections_do_not_fail_continuations() {
    use std::time::Duration;
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    const N: usize = 10;
    for rst in [false, true] {
        let m = RouterMock::builder()
            .idle_close(Duration::from_millis(300), rst)
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(
                Route::contains("POST", "/idx/_search").reply(Reply::ok(page(
                    "s1",
                    &[hit("1", r#"{"n":1}"#)],
                    N,
                ))),
            )
            .route(
                Route::exact("POST", "/_search/scroll").replies(
                    (2..=N)
                        .map(|i| Reply::ok(page("s1", &[hit(&i.to_string(), r#"{"n":1}"#)], N)))
                        .chain([Reply::ok(page("s1", &[], N))]),
                ),
            )
            .fallback(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#))
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("d.json");
        let o = run_async(
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                out.to_str().unwrap().into(),
                "--limit".into(),
                "1".into(),
                "--throttleInterval".into(),
                "600".into(),
                "--bufferSize".into(),
                "1".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert!(o.status.success(), "rst={rst}: {err}");
        assert_eq!(
            std::fs::read_to_string(&out).unwrap().lines().count(),
            N,
            "rst={rst}: {err}"
        );
        assert!(
            m.idle_closed() >= 2,
            "rst={rst}: the LB closed idle connections ({} closes, {} connections, requests {:?})",
            m.idle_closed(),
            m.connections(),
            m.requests()
                .iter()
                .map(|r| (r.conn, r.path.clone()))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "rst={rst}: idle closes {}, connections {}",
            m.idle_closed(),
            m.connections()
        );
    }
}

// L-444 (TST-BRK-SRV-001, Q3): bulk items keep failing (a 429 storm longer than the retry
// budget, a flood-stage `cluster_block_exception` 429, `index.blocks.write` 403 items). By
// default the run continues past them but exits 3 ("finished with failed writes"); only an
// explicit --ignore-es-write-errors (or --ignore-errors) gives 0; `false` stops with 1.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_writes_exit_non_zero_unless_accepted_explicitly() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        format!("{}\n{}\n", hit("1", "{\"a\":1}"), hit("2", "{\"a\":2}")),
    )
    .unwrap();
    let item = |status: u16, ty: &str, reason: &str| {
        format!(
            r#"{{"index":{{"status":{status},"error":{{"type":"{ty}","reason":"{reason}"}}}}}}"#
        )
    };
    let cases = [
        (
            "429 storm",
            item(429, "es_rejected_execution_exception", "rejected execution"),
        ),
        (
            "flood stage",
            item(
                429,
                "cluster_block_exception",
                "index [dst] blocked by: [TOO_MANY_REQUESTS/12/disk usage exceeded flood-stage watermark, index has read-only-allow-delete block];",
            ),
        ),
        (
            "write block",
            item(
                403,
                "cluster_block_exception",
                "index [dst] blocked by: [FORBIDDEN/8/index write (api)];",
            ),
        ),
    ];
    for (label, failing) in cases {
        let body = format!(
            r#"{{"took":1,"errors":true,"items":[{{"index":{{"status":201}}}},{failing}]}}"#
        );
        let retry_body = format!(r#"{{"took":1,"errors":true,"items":[{failing}]}}"#);
        for (flags, code) in [
            (vec![], 3),
            (vec!["--ignore-es-write-errors"], 0),
            (vec!["--ignore-errors"], 0),
            (vec!["--ignore-es-write-errors", "false"], 1),
        ] {
            let m = RouterMock::builder()
                .route(Route::exact("GET", "/").reply(Reply::ok(info)))
                .route(
                    Route::contains("POST", "/_bulk")
                        .replies([Reply::ok(body.clone()), Reply::ok(retry_body.clone())]),
                )
                .fallback(Reply::ok(
                    r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
                ))
                .start()
                .await;
            let mut args: Vec<String> = vec![
                "--input".into(),
                input.to_str().unwrap().into(),
                "--output".into(),
                format!("{}/dst", m.url()),
                "--noRefresh".into(),
                "--retryAttempts".into(),
                "1".into(),
                "--retryDelay".into(),
                "1".into(),
            ];
            args.extend(flags.iter().map(|f| f.to_string()));
            let o = run_async(args, vec![]).await;
            let err = stderr(&o);
            assert_eq!(o.status.code(), Some(code), "{label} {flags:?}: {err}");
            assert!(
                err.contains("cluster_block_exception")
                    || err.contains("es_rejected_execution_exception"),
                "{label}: {err}"
            );
            if code == 3 {
                assert!(
                    err.contains("Failed Writes: 1")
                        && err.contains("1 document(s) were not written")
                        && err.contains("--ignore-es-write-errors"),
                    "{label}: {err}"
                );
                // L-804b (P5): a dominant block is named, with what to do about it
                let (disk, block) = (
                    err.contains("flood-stage watermark: free disk space"),
                    err.contains("refused by an index or cluster block"),
                );
                match label {
                    "flood stage" => assert!(disk && !block, "{label}: {err}"),
                    "write block" => assert!(block && !disk, "{label}: {err}"),
                    _ => assert!(!disk && !block, "{label}: {err}"),
                }
            }
            if code == 0 {
                assert!(err.contains("Failed Writes: 1"), "{label}: {err}");
            }
        }
    }
}

// L-445 (TST-BRK-SRV-006): a response larger than --maxResponseMb (default 256) is refused
// while reading, with or without Content-Length, with a hint to lower --limit; it is not
// retried (the same request would return the same body). The 512 MB cgroup run with an
// endless body is scripts/res/huge-response.sh.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_responses_fail_with_a_limit_hint() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let big = "x".repeat(1_200_000);
    let body = page("s1", &[hit("1", &format!("{{\"blob\":\"{big}\"}}"))], 1);
    for chunked in [false, true] {
        let reply = if chunked {
            Reply::ok(&body).chunked()
        } else {
            Reply::ok(&body)
        };
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::contains("POST", "/idx/_search").reply(reply))
            .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(page("s1", &[], 1))))
            .fallback(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#))
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("d.json");
        let args = |cap: &str| -> Vec<String> {
            vec![
                "--input".into(),
                format!("{}/idx", m.url()),
                "--output".into(),
                out.to_str().unwrap().into(),
                "--maxResponseMb".into(),
                cap.into(),
                "--overwrite".into(),
                "--retryDelay".into(),
                "1".into(),
            ]
        };
        let o = run_async(args("1"), vec![]).await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "chunked={chunked}: {err}");
        assert!(
            err.contains("larger than 1 MB (--maxResponseMb)") && err.contains("lower --limit"),
            "chunked={chunked}: {err}"
        );
        let searches = m
            .requests()
            .iter()
            .filter(|r| r.path.starts_with("/idx/_search"))
            .count();
        assert_eq!(searches, 1, "not retried: {err}");
        assert!(!out.exists());
        // a cap above the page size reads it
        let o = run_async(args("2"), vec![]).await;
        assert!(o.status.success(), "chunked={chunked}: {}", stderr(&o));
    }
}

// L-446 (TST-BRK-SRV-009): the server reports an exact total for a scroll but ends the read
// early (an empty continuation after 2 of 5): the dump fails, names both numbers and commits
// nothing. With --size the total is not a promise and nothing is checked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scroll_total_mismatch_is_an_error() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(info)))
        .route(
            Route::contains("POST", "/idx/_search").reply(Reply::ok(page(
                "s1",
                &[hit("1", r#"{"n":1}"#), hit("2", r#"{"n":1}"#)],
                5,
            ))),
        )
        .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(page("s1", &[], 5))))
        .fallback(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#))
        .start()
        .await;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("d.json");
    let args = |extra: &[&str]| -> Vec<String> {
        let mut a: Vec<String> = vec![
            "--input".into(),
            format!("{}/idx", m.url()),
            "--output".into(),
            out.to_str().unwrap().into(),
            "--overwrite".into(),
        ];
        a.extend(extra.iter().map(|s| s.to_string()));
        a
    };
    let o = run_async(args(&[]), vec![]).await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("retrieved 2 of 5"), "{err}");
    assert!(!out.exists(), "nothing committed");
    let o = run_async(args(&["--size", "2"]), vec![]).await;
    assert!(o.status.success(), "{}", stderr(&o));
}

// L-448 (TST-BRK-IN-004): a metadata dump restored with --type data is refused before
// anything is written (exit 2, "looks like a --type X dump"); a data dump restored with a
// metadata --type is refused the same way; ordinary --sourceOnly documents that merely contain
// keys like `settings` are not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_type_inputs_are_refused_before_writing() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(info)))
        .fallback(Reply::ok(r#"{"took":1,"errors":false,"items":[]}"#))
        .start()
        .await;
    let fx = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/node");
    for engine in ["opensearch-2.19.7", "elasticsearch-8.19.23"] {
        for ty in [
            "mapping",
            "settings",
            "analyzer",
            "alias",
            "index",
            "template",
            "component_template",
            "index_template",
            "policy",
            "script",
        ] {
            let f = fx.join(engine).join(format!("{ty}.json"));
            let o = run_async(
                vec![
                    "--input".into(),
                    f.to_str().unwrap().into(),
                    "--output".into(),
                    format!("{}/dst", m.url()),
                    "--type".into(),
                    "data".into(),
                ],
                vec![],
            )
            .await;
            let err = stderr(&o);
            assert_eq!(o.status.code(), Some(2), "{engine} {ty}: {err}");
            assert!(
                err.contains("looks like a --type") && err.contains("not --type data"),
                "{engine} {ty}: {err}"
            );
        }
        // a data dump with a metadata type
        let f = fx.join(engine).join("data.json");
        let o = run_async(
            vec![
                "--input".into(),
                f.to_str().unwrap().into(),
                "--output".into(),
                format!("{}/dst", m.url()),
                "--type".into(),
                "mapping".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert_ne!(o.status.code(), Some(0), "{engine}: {err}");
        assert!(
            err.contains("looks like a --type data dump"),
            "{engine}: {err}"
        );
    }
    let bulks = m
        .requests()
        .iter()
        .filter(|r| r.path.contains("_bulk") || r.method == "PUT")
        .count();
    assert_eq!(bulks, 0, "nothing written");
    // plain documents that only look similar are restored
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("docs.json");
    std::fs::write(
        &f,
        "{\"user\":{\"settings\":{\"theme\":\"dark\"}}}\n{\"a\":{\"mappings\":{\"x\":1}}}\n",
    )
    .unwrap();
    let out = dir.path().join("out.json");
    let o = run_async(
        vec![
            "--input".into(),
            f.to_str().unwrap().into(),
            "--output".into(),
            out.to_str().unwrap().into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
}

// L-449 (TST-BRK-IN-005): malformed values the splitter passes through (unbalanced brackets,
// a stray `}`, top-level scalars and arrays, NaN/Infinity, trailing commas) are rejected one
// by one with their input line number; the good documents of the same batch are written in
// one bulk request, and the run exits 1. With --ignore-es-write-errors false the first bad line
// stops the run before its batch is sent.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_values_are_rejected_per_line_with_their_number() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let lines = [
        r#"{"_id":"g1","_source":{"a":1}}"#,
        r#"{"a":1]"#,
        r#"{"_id":"g2","_source":{"a":2}}"#,
        "123",
        r#""x""#,
        "null",
        "[1,2]",
        r#"{"_id":"g3","_source":{"a":NaN}}"#,
        r#"{"_id":"g4","_source":{"a":Infinity}}"#,
        r#"{"_id":"g5","_source":{"a":5,}}"#,
        r#"{"_id":"g6","_source":{"a":6}}"#,
        "}",
        r#"{"_id":"g7","_source":{"a":7}}"#,
    ];
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(&input, lines.join("\n") + "\n").unwrap();
    let ok_items = r#"{"took":1,"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}},{"index":{"status":201}},{"index":{"status":201}}]}"#;
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(info)))
        .route(Route::contains("POST", "/_bulk").reply(Reply::ok(ok_items)))
        .fallback(Reply::ok(
            r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
        ))
        .start()
        .await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            format!("{}/dst", m.url()),
            "--ignore-errors".into(),
            "--noRefresh".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    let bulks: Vec<String> = m
        .requests()
        .iter()
        .filter(|r| r.path.ends_with("/_bulk"))
        .map(|r| r.body_str())
        .collect();
    assert_eq!(bulks.len(), 1, "{err}");
    for good in ["g1", "g2", "g6", "g7"] {
        assert!(
            bulks[0].contains(&format!("\"_id\":\"{good}\"")),
            "{}",
            bulks[0]
        );
    }
    assert_eq!(bulks[0].lines().count(), 8, "4 docs: {}", bulks[0]);
    let rejected: Vec<&str> = err
        .lines()
        .filter(|l| l.contains("rejected document"))
        .collect();
    assert_eq!(rejected.len(), 9, "{err}");
    for n in [2, 4, 5, 6, 7, 8, 9, 10, 12] {
        assert!(
            err.contains(&format!("rejected document {n} of the input")),
            "line {n}: {err}"
        );
    }
    // strict: the first bad line stops the run and names it; that batch is not sent
    let m2 = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(info)))
        .route(Route::contains("POST", "/_bulk").reply(Reply::ok(ok_items)))
        .start()
        .await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            format!("{}/dst", m2.url()),
            "--noRefresh".into(),
            "--ignore-es-write-errors".into(),
            "false".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("rejected document 2 of the input"), "{err}");
    assert_eq!(
        m2.requests()
            .iter()
            .filter(|r| r.path.ends_with("/_bulk"))
            .count(),
        0
    );
}

// L-450 (BRK-IN-007): a giant single value stops the run with its position and
// a --maxDocMb hint instead of being buffered whole; nothing of that batch is sent.
#[tokio::test(flavor = "multi_thread")]
async fn giant_documents_are_refused_with_a_size_hint() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    let big = "x".repeat(3 << 20);
    std::fs::write(
        &input,
        format!(
            "{}\n{{\"_id\":\"big\",\"_source\":{{\"v\":\"{big}\"}}}}\n{}\n",
            r#"{"_id":"g1","_source":{"a":1}}"#, r#"{"_id":"g2","_source":{"a":2}}"#
        ),
    )
    .unwrap();
    let ok = r#"{"took":1,"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}},{"index":{"status":201}}]}"#;
    let mock = || {
        RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::contains("POST", "/_bulk").reply(Reply::ok(ok)))
            .fallback(Reply::ok(
                r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
            ))
            .start()
    };
    let m = mock().await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            format!("{}/dst", m.url()),
            "--maxDocMb".into(),
            "1".into(),
            "--noRefresh".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(err.contains("document 2 of the input"), "{err}");
    assert!(err.contains("larger than 1 MB"), "{err}");
    assert!(err.contains("--maxDocMb"), "{err}");
    let bulks = m
        .requests()
        .iter()
        .filter(|r| r.path.ends_with("/_bulk"))
        .count();
    assert_eq!(bulks, 0, "{err}");
    // with the default cap (100 MB) the same input goes through
    let m2 = mock().await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            format!("{}/dst", m2.url()),
            "--noRefresh".into(),
        ],
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
}

// L-452 (BRK-CLI-003): rename rules that map several source indices to one target
// are refused (nothing written) unless --allowMerge is given.
#[tokio::test(flavor = "multi_thread")]
async fn many_to_one_renames_are_refused_unless_allowed() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let ok2 =
        r#"{"took":1,"errors":false,"items":[{"index":{"status":201}},{"index":{"status":201}}]}"#;
    let dst = || {
        RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::contains("POST", "/_bulk").reply(Reply::ok(ok2)))
            .fallback(Reply::ok(
                r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
            ))
            .start()
    };
    let bulks = |m: &RouterMock| {
        m.requests()
            .iter()
            .filter(|r| r.path.ends_with("/_bulk"))
            .map(|r| r.body_str())
            .collect::<Vec<_>>()
    };
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        "{\"_index\":\"a-1\",\"_id\":\"1\",\"_source\":{\"v\":\"a\"}}\n{\"_index\":\"b-1\",\"_id\":\"1\",\"_source\":{\"v\":\"b\"}}\n",
    )
    .unwrap();
    let rule = "^(a|b)-(.*):x-$2";
    // file -> cluster: refused before the batch is sent
    let m = dst().await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            m.url(),
            "--renameIndex".into(),
            rule.into(),
            "--noRefresh".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("'a-1' and 'b-1'") && err.contains("'x-1'"),
        "{err}"
    );
    assert!(err.contains("--allowMerge"), "{err}");
    assert!(
        !err.contains("rejected document"),
        "fatal, not a per-document rejection: {err}"
    );
    assert!(bulks(&m).is_empty(), "{err}");
    // --allowMerge merges on purpose
    let m = dst().await;
    let o = run_async(
        vec![
            "--input".into(),
            input.to_str().unwrap().into(),
            "--output".into(),
            m.url(),
            "--renameIndex".into(),
            rule.into(),
            "--allowMerge".into(),
            "--noRefresh".into(),
        ],
        vec![],
    )
    .await;
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let b = bulks(&m);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].matches("\"_index\":\"x-1\"").count(), 2, "{}", b[0]);
    // cluster -> cluster: the source indices are listed and checked before reading
    let src = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(info)))
        .route(Route::exact("GET", "/_all/_alias").reply(Reply::ok(
            r#"{"a-1":{"aliases":{}},"b-1":{"aliases":{}},"c":{"aliases":{}}}"#,
        )))
        .fallback(Reply::json(500, r#"{"error":"unexpected"}"#))
        .start()
        .await;
    let m = dst().await;
    let o = run_async(
        vec![
            "--input".into(),
            src.url(),
            "--output".into(),
            m.url(),
            "--renameIndex".into(),
            rule.into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(2), "{err}");
    assert!(
        err.contains("'a-1' and 'b-1'") && err.contains("Nothing was written"),
        "{err}"
    );
    assert!(
        src.requests().iter().all(|r| !r.path.contains("_search")),
        "nothing read"
    );
    assert!(bulks(&m).is_empty());
}

// L-454 (BRK-SIG-001): SIGTERM (what podman stop, Kubernetes and systemd send) is handled
// like SIGINT: the scroll is cleared, staged split parts are removed, and the exit status
// is 128 + the signal (143 / 130).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sigterm_and_sigint_stop_cleanly_with_128_plus_signal() {
    use std::time::{Duration, Instant};
    use support::router_mock::{Fault, Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let page = r#"{"_scroll_id":"s1","hits":{"total":{"value":1000000,"relation":"eq"},"hits":[{"_index":"i","_id":"1","_source":{"a":1}},{"_index":"i","_id":"2","_source":{"a":2}}]}}"#;
    let leftovers = |d: &std::path::Path| -> Vec<String> {
        std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("out") || n.contains(".part-"))
            .collect()
    };
    let wait_exit = |child: &mut std::process::Child| {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(s) = child.try_wait().unwrap() {
                return s;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                panic!("no exit within 20 s of the signal");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let signal = |pid: u32, sig: &str| {
        assert!(
            std::process::Command::new("kill")
                .args([sig, &pid.to_string()])
                .status()
                .unwrap()
                .success()
        );
    };
    use std::os::unix::process::ExitStatusExt;
    // cluster -> split file output
    for (sig, code) in [("-TERM", 143), ("-INT", 130)] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::prefix("POST", "/i/_search").reply(Reply::ok(page)))
            .route(
                Route::exact("POST", "/_search/scroll")
                    .reply(Reply::ok(page).with_fault(Fault::Delay(Duration::from_millis(30)))),
            )
            .route(
                Route::prefix("DELETE", "/_search/scroll")
                    .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
            )
            .start()
            .await;
        let dir = tempfile::tempdir().unwrap();
        let mut child = bin()
            .args([
                "--input",
                &format!("{}/i", m.url()),
                "--output",
                dir.path().join("out.json").to_str().unwrap(),
                "--maxRows",
                "2",
                "--limit",
                "2",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while m
            .requests()
            .iter()
            .filter(|r| r.path == "/_search/scroll")
            .count()
            < 5
        {
            assert!(Instant::now() < deadline, "dump never got going");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!leftovers(dir.path()).is_empty(), "parts are staged");
        signal(child.id(), sig);
        let status = tokio::task::block_in_place(|| wait_exit(&mut child));
        let mut err = String::new();
        std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut err).unwrap();
        assert_eq!(
            status.code(),
            Some(code),
            "{sig}: {status:?} {:?} {err}",
            status.signal()
        );
        assert!(err.contains("interrupted"), "{err}");
        assert!(
            leftovers(dir.path()).is_empty(),
            "{sig}: {:?}",
            leftovers(dir.path())
        );
        assert!(
            m.requests()
                .iter()
                .any(|r| r.method == "DELETE" && r.path.starts_with("/_search/scroll")),
            "{sig}: scroll cleared"
        );
    }
    // file (a FIFO whose writer stays open) -> split file output
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("in.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let mut child = bin()
        .args([
            "--input",
            fifo.to_str().unwrap(),
            "--output",
            dir.path().join("out.json").to_str().unwrap(),
            "--maxRows",
            "2",
            "--limit",
            "2",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let writer = {
        let fifo = fifo.clone();
        std::thread::spawn(move || {
            let mut w = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
            for i in 0..10 {
                let _ = std::io::Write::write_all(
                    &mut w,
                    format!("{{\"_index\":\"i\",\"_id\":\"{i}\",\"_source\":{{}}}}\n").as_bytes(),
                );
            }
            std::thread::sleep(Duration::from_secs(4)); // keep the input open
        })
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while leftovers(dir.path()).len() < 2 {
        assert!(Instant::now() < deadline, "restore never got going");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    signal(child.id(), "-TERM");
    let status = tokio::task::block_in_place(|| wait_exit(&mut child));
    let mut err = String::new();
    std::io::Read::read_to_string(child.stderr.as_mut().unwrap(), &mut err).unwrap();
    assert_eq!(status.code(), Some(143), "{status:?} {err}");
    assert!(
        leftovers(dir.path()).is_empty(),
        "{:?}",
        leftovers(dir.path())
    );
    writer.join().unwrap();
}

// L-455 (BRK-XV-002): hits without _source (mapping _source.enabled: false) stop the dump
// with "documents cannot be fully dumped" unless --allowMissingSource or a searchBody that
// filters _source itself. L-455c: so do hits whose _source is empty ({}).
#[tokio::test(flavor = "multi_thread")]
async fn hits_without_source_are_refused_unless_allowed() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let page = r#"{"_scroll_id":"s1","hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_index":"nosrc","_id":"1","_score":1.0}]}}"#;
    // L-455c: a hit whose _source is `{}` (an empty document, or a filter that keeps nothing)
    let empty_page = r#"{"_scroll_id":"s1","hits":{"total":{"value":1,"relation":"eq"},"hits":[{"_index":"nosrc","_id":"e-7","_score":1.0,"_source":{ }}]}}"#;
    let empty = r#"{"_scroll_id":"s1","hits":{"total":{"value":1,"relation":"eq"},"hits":[]}}"#;
    let mock = |page: &'static str| {
        RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::prefix("POST", "/nosrc/_search").reply(Reply::ok(page)))
            .route(Route::exact("POST", "/_search/scroll").reply(Reply::ok(empty)))
            .route(
                Route::prefix("DELETE", "/_search/scroll")
                    .reply(Reply::ok(r#"{"succeeded":true,"num_freed":1}"#)),
            )
            .start()
    };
    let dir = tempfile::tempdir().unwrap();
    // L-455a: stored_fields without _source makes the cluster leave _source out; still
    // refused by default, and the error names the search body, not the mapping.
    // L-455b (behaviour change): a body that turns _source off is refused too unless
    // --allowMissingSource: `"_source": false` before the first search, a filter that
    // leaves no _source (`[]`) at the first such hit.
    // L-455c (behaviour change): an empty _source counts as missing. A filter that
    // excludes every field (`{"excludes":["*"]}`, `"exclude":"**"`) is refused before the
    // first search (it used to write `{}`); otherwise the first hit whose _source is `{}`
    // stops the run naming its _id and the likely cause (the filter, or a document that is
    // really empty in the source index, which needs the flag too)
    let stored = r#"{"stored_fields":["x"]}"#;
    let off = r#"{"query":{"match_all":{}},"_source":false}"#;
    let empty_list = r#"{"_source":[]}"#;
    let excl_all = r#"{"_source":{"excludes":["*"]}}"#;
    let excl_all_str = r#"{"_source":{"includes":["a"],"exclude":"**"}}"#;
    let incl_none = r#"{"_source":{"includes":["nope"]}}"#;
    #[derive(PartialEq, Debug)]
    enum Want {
        Ok,
        Mapping,
        Stored,
        Off,
        Filter,
        ExcludesAll,
        EmptyDoc,
        EmptyFiltered,
    }
    for (n, (extra, want, page)) in [
        (vec![], Want::Mapping, page),
        (vec!["--searchBody", stored], Want::Stored, page),
        (
            vec!["--searchBody", stored, "--allowMissingSource"],
            Want::Ok,
            page,
        ),
        (vec!["--allowMissingSource"], Want::Ok, page),
        (vec!["--searchBody", off], Want::Off, page),
        (
            vec!["--searchBody", off, "--allowMissingSource"],
            Want::Ok,
            page,
        ),
        (vec!["--searchBody", empty_list], Want::Filter, page),
        (vec!["--searchBody", excl_all], Want::ExcludesAll, page),
        (vec!["--searchBody", excl_all_str], Want::ExcludesAll, page),
        (
            vec!["--searchBody", excl_all, "--allowMissingSource"],
            Want::Ok,
            page,
        ),
        (vec![], Want::EmptyDoc, empty_page),
        (vec!["--allowMissingSource"], Want::Ok, empty_page),
        (
            vec!["--searchBody", incl_none],
            Want::EmptyFiltered,
            empty_page,
        ),
        (
            vec!["--searchBody", incl_none, "--allowMissingSource"],
            Want::Ok,
            empty_page,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let m = mock(page).await;
        let out = dir.path().join(format!("out-{n}.json"));
        let mut args = vec![
            "--input".to_string(),
            format!("{}/nosrc", m.url()),
            "--output".into(),
            out.to_str().unwrap().into(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        let o = run_async(args, vec![]).await;
        let err = stderr(&o);
        if want == Want::Ok {
            assert_eq!(o.status.code(), Some(0), "{extra:?}: {err}");
            assert!(
                std::fs::read_to_string(&out)
                    .unwrap()
                    .contains(if page == empty_page {
                        "\"_id\":\"e-7\""
                    } else {
                        "\"_id\":\"1\""
                    })
            );
            continue;
        }
        assert_eq!(o.status.code(), Some(1), "{extra:?}: {err}");
        assert!(err.contains("documents cannot be fully dumped"), "{err}");
        assert!(err.contains("--allowMissingSource"), "{err}");
        assert!(!out.exists(), "nothing written");
        let searched = m.requests().iter().any(|r| r.path.contains("_search"));
        match want {
            Want::Mapping => {
                assert!(
                    err.contains("'nosrc'")
                        && err.contains("its mapping has _source.enabled: false"),
                    "{err}"
                );
                assert!(!err.contains("stored_fields"), "{err}");
            }
            Want::Stored => {
                assert!(
                    err.contains("'nosrc'")
                        && err.contains("the search body sets stored_fields without _source")
                        && err.contains("\"_source\": true"),
                    "{err}"
                );
                assert!(!err.contains("_source.enabled"), "{err}");
            }
            Want::Off => {
                assert!(
                    err.contains("the search body sets \"_source\": false")
                        && err.contains("remove \"_source\": false from the search body"),
                    "{err}"
                );
                assert!(!err.contains("_source.enabled"), "{err}");
                assert!(!searched, "refused before the first search");
            }
            Want::Filter => {
                assert!(
                    err.contains("'nosrc'")
                        && err.contains("the search body's _source filter leaves it out")
                        && err.contains("remove the _source filter from the search body"),
                    "{err}"
                );
                assert!(!err.contains("_source.enabled"), "{err}");
            }
            Want::ExcludesAll => {
                assert!(
                    err.contains("the search body's _source filter excludes every field")
                        && err.contains("empty _source ({})")
                        && err.contains("remove the _source filter from the search body"),
                    "{err}"
                );
                assert!(!searched, "refused before the first search");
            }
            Want::EmptyDoc => {
                assert!(
                    err.contains("document 'e-7' in index 'nosrc' has an empty _source ({})")
                        && err.contains("the document is empty in the source index"),
                    "{err}"
                );
                assert!(!err.contains("search body"), "{err}");
            }
            Want::EmptyFiltered => {
                assert!(
                    err.contains("document 'e-7' in index 'nosrc' has an empty _source ({})")
                        && err
                            .contains("the search body's _source filter leaves none of its fields"),
                    "{err}"
                );
            }
            Want::Ok => unreachable!(),
        }
    }
}

// L-456 (BRK-XV-003): documents from a data stream's backing index go back to the stream
// with --bulkAction create; with index the run stops before writing.
#[tokio::test(flavor = "multi_thread")]
async fn data_stream_documents_need_create_and_target_the_stream() {
    use support::router_mock::{Reply, Route, RouterMock};
    let info = r#"{"version":{"distribution":"opensearch","number":"2.19.0"}}"#;
    let ok = r#"{"took":1,"errors":false,"items":[{"create":{"status":201}},{"create":{"status":201}}]}"#;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.json");
    std::fs::write(
        &input,
        "{\"_index\":\".ds-logs-a-2026.10.30-000001\",\"_id\":\"1\",\"_source\":{\"@timestamp\":\"2026-10-30T00:00:00Z\"}}\n{\"_index\":\".ds-logs-a-2026.10.30-000002\",\"_id\":\"2\",\"_source\":{\"@timestamp\":\"2026-10-30T00:00:01Z\"}}\n",
    )
    .unwrap();
    for (action, ok_run) in [("index", false), ("create", true)] {
        let m = RouterMock::builder()
            .route(Route::exact("GET", "/").reply(Reply::ok(info)))
            .route(Route::contains("POST", "/_bulk").reply(Reply::ok(ok)))
            .fallback(Reply::ok(
                r#"{"_shards":{"total":1,"successful":1,"failed":0}}"#,
            ))
            .start()
            .await;
        let o = run_async(
            vec![
                "--input".into(),
                input.to_str().unwrap().into(),
                "--output".into(),
                m.url(),
                "--bulkAction".into(),
                action.into(),
                "--noRefresh".into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        let bulks: Vec<String> = m
            .requests()
            .iter()
            .filter(|r| r.path.ends_with("/_bulk"))
            .map(|r| r.body_str())
            .collect();
        if ok_run {
            assert_eq!(o.status.code(), Some(0), "{err}");
            assert_eq!(bulks.len(), 1);
            assert_eq!(
                bulks[0].matches(r#"{"create":{"_index":"logs-a""#).count(),
                2,
                "{}",
                bulks[0]
            );
            assert!(!bulks[0].contains(".ds-"), "{}", bulks[0]);
        } else {
            assert_eq!(o.status.code(), Some(1), "{err}");
            assert!(
                err.contains("data stream 'logs-a'") && err.contains("--bulkAction create"),
                "{err}"
            );
            assert!(!err.contains("rejected document"), "{err}");
            assert!(bulks.is_empty(), "{err}");
        }
    }
}

/// L-508 (TST-SUP-020): hostile compressed input. Every case either restores exactly the
/// documents it holds or stops with exit 1 and a clear error, and writes no output file.
#[test]
fn hostile_compressed_input_fails_cleanly() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let docs = "{\"_id\":\"a\",\"_source\":{\"n\":1}}\n{\"_id\":\"b\",\"_source\":{\"n\":2}}\n";
    let zst = |s: &[u8]| zstd::stream::encode_all(s, 3).unwrap();
    let gz = |s: &[u8]| {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(s).unwrap();
        e.finish().unwrap()
    };
    // zstd frame by hand: magic, FHD 0 (no FCS/checksum/dict), window descriptor, blocks
    let frame = |wd: u8, blocks: &[(u8, u32, &[u8])]| {
        let mut f = vec![0x28, 0xb5, 0x2f, 0xfd, 0x00, wd];
        for (i, (ty, size, body)) in blocks.iter().enumerate() {
            let last = u32::from(i + 1 == blocks.len());
            let h = last | (u32::from(*ty) << 1) | (size << 3);
            f.extend_from_slice(&h.to_le_bytes()[..3]);
            f.extend_from_slice(body);
        }
        f
    };
    let full = zst(docs.as_bytes());
    let skippable = [&[0x50, 0x2a, 0x4d, 0x18, 4, 0, 0, 0][..], b"junk"].concat();
    let (first, second) = docs.split_at(docs.find('\n').unwrap() + 1);
    let huge_window = frame(0xa8, &[(0, 1, b"{")]); // window 2^31 > cap 2^27
    let mut members = gz(first.as_bytes());
    members.extend(gz(second.as_bytes()));
    // a 64 MiB value from 512 RLE blocks of 128 KiB (about 2 KiB on disk: 32000:1)
    // the value continues from the first frame into the second
    let mut bomb = frame(0x38, &[(0, 32, br#"{"_id":"big","_source":{"v":"xxx"#)]);
    let mut rle: Vec<(u8, u32, &[u8])> = vec![(1, 128 << 10, b"x"); 512];
    rle.push((0, 3, b"\"}}"));
    bomb.extend(frame(0x38, &rle));
    let cases: Vec<(&str, Vec<u8>, Result<usize, &str>)> = vec![
        (
            "concat.zst",
            [
                zst(first.as_bytes()),
                skippable.clone(),
                zst(second.as_bytes()),
            ]
            .concat(),
            Ok(2),
        ),
        ("multi.json.gz", members.clone(), Ok(2)),
        (
            "truncated.zst",
            full[..full.len() - 5].to_vec(),
            Err("truncated or corrupt input"),
        ),
        (
            "garbage.zst",
            [full.clone(), b"GARBAGE!".to_vec()].concat(),
            Err("truncated or corrupt input"),
        ),
        (
            "garbage.json.gz",
            [members.clone(), b"GARBAGE!".to_vec()].concat(),
            Err("truncated or corrupt input"),
        ),
        (
            "truncated.json.gz",
            members[..members.len() - 6].to_vec(),
            Err("truncated or corrupt input"),
        ),
        ("window.zst", huge_window, Err("window over 2^27")),
        ("bomb.zst", bomb, Err("--maxDocMb")),
    ];
    for (name, bytes, want) in cases {
        let input = dir.path().join(name);
        std::fs::write(&input, &bytes).unwrap();
        let out = dir.path().join(format!("{name}.out.json"));
        let o = run(&[
            "--input",
            input.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
            "--maxDocMb",
            "16",
        ]);
        let err = stderr(&o);
        match want {
            Ok(n) => {
                assert_eq!(o.status.code(), Some(0), "{name}: {err}");
                let got = std::fs::read_to_string(&out).unwrap();
                assert_eq!(got.lines().count(), n, "{name}: {got}");
            }
            Err(needle) => {
                assert_eq!(o.status.code(), Some(1), "{name}: {err}");
                assert!(err.contains(needle), "{name}: {err}");
                assert!(!out.exists(), "{name}: output written");
                assert!(!err.contains("panicked"), "{name}: {err}");
            }
        }
    }
}

/// L-646 (TST-BRK-CLI-008): scheme-less and mistyped cluster URLs. A
/// scheme-less output with `user:password@` is refused before anything is
/// written (it would put the password in a file name); mistyped schemes are
/// usage errors; `host:port` "files" warn; uppercase schemes are clusters;
/// no message quotes a password.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn l646_scheme_less_and_mistyped_urls() {
    let cwd = tempfile::tempdir().unwrap();
    let input = cwd.path().join("in.json");
    std::fs::write(&input, format!("{}\n", hit("1", "{}"))).unwrap();
    let input = input.to_str().unwrap().to_string();
    let run_in = |args: Vec<String>| {
        let dir = cwd.path().to_path_buf();
        async move {
            tokio::task::spawn_blocking(move || {
                bin().current_dir(dir).args(args).output().expect("spawn")
            })
            .await
            .unwrap()
        }
    };
    // (output, exit code, expected stderr text)
    let cases: &[(&str, i32, &str)] = &[
        (
            "elastic:FAKE-pw-646@localhost:9200",
            2,
            "name holds the password",
        ),
        (
            "elastic:FAKE/pw-646@localhost:9200",
            2,
            "name holds the password",
        ),
        (
            "http:/elastic:FAKE-pw-646@localhost:9200/i",
            2,
            "mistyped URL",
        ),
        ("HTTP:/localhost:9200/i", 2, "mistyped URL"),
        ("http//localhost:9200/i", 2, "mistyped URL"),
        (
            "htp://elastic:FAKE-pw-646@localhost:9200/i",
            2,
            "unsupported scheme 'htp://'",
        ),
        (
            " http://elastic:FAKE-pw-646@localhost:9200/i",
            2,
            "whitespace",
        ),
        ("localhost:9200", 0, "is treated as a FILE path"),
        ("./elastic:FAKE-pw-646@x.json", 0, ""),
    ];
    for (out, code, want) in cases {
        let o = run_in(vec![
            "--input".into(),
            input.clone(),
            "--output".into(),
            (*out).into(),
        ])
        .await;
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(*code), "{out:?}: {err}");
        assert!(err.contains(want), "{out:?}: {err}");
        assert!(!err.contains("FAKE"), "{out:?}: {err}");
        assert!(!stdout(&o).contains("FAKE"), "{out:?}");
        if *code == 2 {
            assert!(
                err.contains("http://") && err.contains("./"),
                "{out:?}: {err}"
            );
        }
    }
    // only the explicit ./ file and the host:port file were written
    let mut names: Vec<String> = std::fs::read_dir(cwd.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["elastic:FAKE-pw-646@x.json", "in.json", "localhost:9200"]
    );

    // scheme-less input with credentials: warned, masked, read as a file
    let o = run_in(vec![
        "--input".into(),
        "elastic:FAKE-pw-646@localhost:9200/idx".into(),
        "--output".into(),
        "$".into(),
    ])
    .await;
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains("elastic:***@localhost:9200/idx is treated as a FILE path"),
        "{err}"
    );
    assert!(!err.contains("FAKE"), "{err}");

    // a mistyped input scheme is a usage error before anything is read
    let o = run_in(vec![
        "--input".into(),
        "http:/localhost:1/idx".into(),
        "--output".into(),
        "$".into(),
    ])
    .await;
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));

    // uppercase schemes are clusters
    let mock = MockEs::serve(scroll_script(&[hit("1", r#"{"a":1}"#)])).await;
    let url = mock.input_url("idx").replacen("http://", "HTTP://", 1);
    let o = run_async(
        vec![
            "--input".into(),
            url,
            "--output".into(),
            "$".into(),
            "--quiet".into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains(r#""_id":"1""#), "{}", stdout(&o));
}

/// L-642a: the same-target check compares the input index with the output index as
/// written to, i.e. with `--prefix`/`--suffix` applied. Different after the affix is
/// allowed (it gets as far as the unreachable cluster); the same after the affix is still
/// refused as a usage error before any network I/O.
#[test]
fn l642a_same_target_check_applies_prefix_and_suffix() {
    let host = "http://127.0.0.1:1";
    let at = |i: &str| format!("{host}/{i}");
    let refused: Vec<Vec<String>> = vec![
        // the affix maps the output back onto the input index
        vec![at("p-idx"), at("idx"), "--prefix".into(), "p-".into()],
        vec![at("idx-s"), at("idx"), "--suffix=-s".into()],
        vec![
            at("p-idx-s"),
            at("idx"),
            "--prefix".into(),
            "p-".into(),
            "--suffix=-s".into(),
        ],
        // no affix: same as before
        vec![at("idx"), at("idx")],
        vec![host.into(), host.into()],
    ];
    for a in refused {
        let mut args = vec!["--input", &a[0], "--output", &a[1]];
        args.extend(a[2..].iter().map(String::as_str));
        let o = bin().args(&args).output().expect("spawn");
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {err}");
        assert!(err.contains("same file or index"), "{args:?}: {err}");
        assert_eq!(
            err.contains("(after --prefix/--suffix)"),
            a.len() > 2,
            "{args:?}: {err}"
        );
    }
    let allowed: Vec<Vec<String>> = vec![
        // the affix makes the target a different index
        vec![at("idx"), at("idx"), "--prefix".into(), "p-".into()],
        vec![at("idx"), at("idx"), "--suffix=-copy".into()],
        vec![at("p-idx"), at("p-idx"), "--prefix".into(), "p-".into()],
        vec![host.into(), host.into(), "--suffix=-copy".into()],
    ];
    for a in allowed {
        let mut args = vec!["--input", &a[0], "--output", &a[1]];
        args.extend(a[2..].iter().map(String::as_str));
        args.extend(["--retryAttempts", "1", "--noImplicitRetry"]);
        let o = bin().args(&args).output().expect("spawn");
        let err = stderr(&o);
        assert!(!err.contains("same file or index"), "{args:?}: {err}");
        assert_ne!(o.status.code(), Some(2), "{args:?}: {err}");
    }
}

/// L-642b: a host-only output writes each source index to its own name (after --prefix/--suffix
/// and the rename rules), so that resolved name is what is compared with the input index.
#[test]
fn l642b_host_only_output_resolves_the_target_index() {
    let host = "http://127.0.0.1:1";
    let at = |i: &str| format!("{host}/{i}");
    let args_of = |a: &[String]| {
        let mut v = vec![
            "--input".to_string(),
            a[0].clone(),
            "--output".into(),
            host.into(),
        ];
        v.extend(a[1..].iter().cloned());
        v
    };
    // (input index and flags, expected wording after "same file or index")
    let refused: Vec<(Vec<String>, &str)> = vec![
        (vec![at("idx")], ""),
        (vec![at("other,idx")], ""),
        (vec![at("logs-*")], ""),
        (
            vec![at("idx"), "--renameIndex".into(), "^nomatch$:x".into()],
            " (after --renamePrefix/--renameIndex)",
        ),
        (
            vec![
                at("p-idx"),
                "--renameIndex".into(),
                "^p-(.*)$:$1".into(),
                "--prefix".into(),
                "p-".into(),
            ],
            " (after --renamePrefix/--renameIndex and --prefix/--suffix)",
        ),
    ];
    for (a, wording) in refused {
        let args = args_of(&a);
        let o = bin().args(&args).output().expect("spawn");
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(2), "{args:?}: {err}");
        assert!(
            err.contains(&format!("same file or index{wording};")),
            "{args:?}: {err}"
        );
    }
    let allowed: Vec<Vec<String>> = vec![
        vec![at("idx"), "--prefix".into(), "p-".into()],
        vec![at("idx"), "--suffix=-copy".into()],
        vec![at("idx"), "--renameIndex".into(), "^idx$:idx2".into()],
        vec![at("idx"), "--renamePrefix".into(), "id:new-id".into()],
        vec![at("logs-*"), "--suffix=-copy".into()],
        // which indices a wildcard matches is only known at run time
        vec![
            at("logs-*"),
            "--renameIndex".into(),
            "^logs-(.*)$:old-$1".into(),
        ],
        vec![
            at("idx"),
            "--prefix".into(),
            "p-".into(),
            "--suffix=-s".into(),
        ],
    ];
    for a in allowed {
        let mut args = args_of(&a);
        args.extend(["--retryAttempts", "1", "--noImplicitRetry"].map(String::from));
        let o = bin().args(&args).output().expect("spawn");
        let err = stderr(&o);
        assert!(!err.contains("same file or index"), "{args:?}: {err}");
        assert_ne!(o.status.code(), Some(2), "{args:?}: {err}");
    }
}
