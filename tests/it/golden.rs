//! Golden recording (TST-INF-003). `#[ignore]`d: runs only through
//! `scripts/golden/record.sh <profile>` (`IT_RECORD=1`), which writes
//! `tests/golden/<engine>-<version>/<case>/` (or `$IT_GOLDEN_DIR/...` for a re-record check).
//! Replay of the committed files is `tests/golden_replay.rs` (offline, T1).

use crate::harness::*;
use crate::support::golden::{Meta, Recorder, Replay, golden_root};
use serde_json::json;

const SRC: &str = "golden_src";
const DST: &str = "golden_dst";

/// 25 deterministic documents in one shard (one shard keeps the scroll order stable).
async fn seed(c: &Cluster) {
    c.delete_index(SRC).await;
    c.ok(
        "PUT",
        SRC,
        Some(json!({
            "settings": {"number_of_shards": 1, "number_of_replicas": 0,
                "analysis": {"analyzer": {"golden_an": {"type": "custom", "tokenizer": "whitespace", "filter": ["lowercase"]}}}},
            "mappings": {"properties": {"k": {"type": "keyword"}, "n": {"type": "long"}, "t": {"type": "text", "analyzer": "golden_an"}, "big": {"type": "keyword"}}}
        })),
    )
    .await;
    let mut nd = String::new();
    for i in 0..25 {
        nd.push_str(&json!({"index": {"_index": SRC, "_id": format!("g{i:02}")}}).to_string());
        nd.push('\n');
        nd.push_str(
            &json!({"k": format!("key{i}"), "n": i, "t": format!("golden doc {i}")}).to_string(),
        );
        nd.push('\n');
    }
    nd.push_str(&json!({"index": {"_index": SRC, "_id": "adv"}}).to_string());
    nd.push('\n');
    nd.push_str(r#"{"k":"unicode-✓-漢字","t":"line\nbreak \"q\"","n":1,"big":123456789012345678901234567890}"#);
    nd.push('\n');
    c.bulk_raw(nd).await;
    c.ok("POST", &format!("{SRC}/_refresh"), None).await;
}

const RESTORE_INPUT: &str = concat!(
    r#"{"_index":"golden_dst","_id":"r1","_source":{"k":"a","n":1}}"#,
    "\n",
    r#"{"_index":"golden_dst","_id":"r2","_source":{"k":"b","n":2}}"#,
    "\n",
    r#"{"_index":"golden_dst","_id":"r3","_source":{"k":"c","n":3,"t":"unicode ✓"}}"#,
    "\n",
    r#"{"_index":"golden_dst","_id":"r4","_source":{"k":"d","n":4}}"#,
    "\n",
);

/// `--handleVersion --versionType external` input: routing and an explicit version.
const VERSION_INPUT: &str = concat!(
    r#"{"_index":"golden_dst","_id":"v1","_version":5,"_routing":"ra","_source":{"k":"a","n":1}}"#,
    "\n",
    r#"{"_index":"golden_dst","_id":"v2","_version":7,"_source":{"k":"b","n":2}}"#,
    "\n",
);

fn input_for(case: &str) -> &'static str {
    match case {
        "bulk-version" => VERSION_INPUT,
        _ => RESTORE_INPUT,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Setup {
    None,
    /// delete golden_dst (restore creates it)
    FreshDst,
    /// golden_dst with golden_src's shape (analyzer restore needs an existing index)
    ShapedDst,
    Alias,
    LegacyTemplate,
    ComposableTemplates,
    Policy,
    Script,
}

struct Case {
    name: &'static str,
    setup: Setup,
    /// Run first, straight against the cluster (`{{URL}}` = cluster), to make `{{DIR}}/input.json`.
    prep: Option<&'static [&'static str]>,
    args: &'static [&'static str],
    output: bool,
    /// Must exit 0 on every engine; otherwise the exit code is recorded (TST-API-002:
    /// "not applicable" is the asserted specific failure, e.g. PIT on OpenSearch 1.3).
    must_pass: bool,
}

const fn case(name: &'static str, args: &'static [&'static str], output: bool) -> Case {
    Case {
        name,
        setup: Setup::None,
        prep: None,
        args,
        output,
        must_pass: true,
    }
}

const CASES: &[Case] = &[
    // E1 info, E2/E3 scroll
    case(
        "data-scroll",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--limit",
            "7",
        ],
        true,
    ),
    // E4/E5 PIT (absent on ES 7.10 OSS and OpenSearch < 2.4: expected failure)
    Case {
        must_pass: false,
        ..case(
            "data-pit",
            &[
                "--input",
                "{{URL}}/golden_src",
                "--output",
                "{{OUT}}",
                "--searchType",
                "pit",
                "--limit",
                "7",
            ],
            true,
        )
    },
    // E6 search_after without PIT
    case(
        "data-search-after",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--searchAfter",
            "--searchBody",
            r#"{"query":{"match_all":{}},"sort":[{"k":"asc"}]}"#,
            "--limit",
            "10",
        ],
        true,
    ),
    // E7 render template
    case(
        "data-template",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--searchWithTemplate",
            "--searchBody",
            r#"{"source":{"query":{"term":{"k":"{{key}}"}}},"params":{"key":"key7"}}"#,
        ],
        true,
    ),
    // E8/E9 bulk + refresh
    Case {
        setup: Setup::FreshDst,
        ..case(
            "data-restore",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--limit",
                "3",
                // 1.1-1: Node's per-batch refresh, so the recording keeps the E9 refresh
                // between batches (the default refreshes once at the end of a short run)
                "--refreshInterval=0",
            ],
            false,
        )
    },
    // TST-API-020: _bulk request shape per action, with routing and external versions, gzip
    // bodies and --maxBulkBytes splits (byte-exact NDJSON in the goldens; a 2xx recorded on
    // every version is the "accepts the gzip body" evidence)
    Case {
        setup: Setup::FreshDst,
        ..case(
            "bulk-create",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--bulkAction",
                "create",
            ],
            false,
        )
    },
    Case {
        setup: Setup::None,
        ..case(
            "bulk-update",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--bulkAction",
                "update",
            ],
            false,
        )
    },
    Case {
        setup: Setup::None,
        ..case(
            "bulk-delete",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--bulkAction",
                "delete",
            ],
            false,
        )
    },
    Case {
        setup: Setup::FreshDst,
        ..case(
            "bulk-version",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--handleVersion",
                "--versionType",
                "external",
            ],
            false,
        )
    },
    Case {
        setup: Setup::FreshDst,
        ..case(
            "bulk-gzip",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--esCompress",
                "--limit",
                "2",
                "--refreshInterval=0",
            ],
            false,
        )
    },
    Case {
        setup: Setup::FreshDst,
        ..case(
            "bulk-split",
            &[
                "--input",
                "{{DIR}}/input.ndjson",
                "--output",
                "{{URL}}/golden_dst",
                "--maxBulkBytes",
                "120b",
            ],
            false,
        )
    },
    // E10/E11/E12 index-scoped metadata, dump and restore
    case(
        "mapping",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--type",
            "mapping",
        ],
        true,
    ),
    case(
        "settings",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--type",
            "settings",
        ],
        true,
    ),
    case(
        "index",
        &[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{OUT}}",
            "--type",
            "index",
        ],
        true,
    ),
    Case {
        setup: Setup::FreshDst,
        prep: Some(&[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{DIR}}/input.json",
            "--type",
            "settings",
        ]),
        ..case(
            "settings-restore",
            &[
                "--input",
                "{{DIR}}/input.json",
                "--output",
                "{{URL}}/golden_dst",
                "--type",
                "settings",
            ],
            false,
        )
    },
    // after settings-restore (the index and its analyzer exist), as a real restore runs
    Case {
        setup: Setup::None,
        prep: Some(&[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{DIR}}/input.json",
            "--type",
            "mapping",
        ]),
        ..case(
            "mapping-restore",
            &[
                "--input",
                "{{DIR}}/input.json",
                "--output",
                "{{URL}}/golden_dst",
                "--type",
                "mapping",
            ],
            false,
        )
    },
    // E13/E21 analyzer: close, settings, open, health wait
    Case {
        setup: Setup::ShapedDst,
        prep: Some(&[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{DIR}}/input.json",
            "--type",
            "analyzer",
        ]),
        ..case(
            "analyzer-restore",
            &[
                "--input",
                "{{DIR}}/input.json",
                "--output",
                "{{URL}}/golden_dst",
                "--type",
                "analyzer",
                "--closeIndex",
            ],
            false,
        )
    },
    // E15 aliases
    Case {
        setup: Setup::Alias,
        ..case(
            "alias",
            &[
                "--input",
                "{{URL}}/golden_src",
                "--output",
                "{{OUT}}",
                "--type",
                "alias",
            ],
            true,
        )
    },
    Case {
        setup: Setup::Alias,
        prep: Some(&[
            "--input",
            "{{URL}}/golden_src",
            "--output",
            "{{DIR}}/input.json",
            "--type",
            "alias",
        ]),
        ..case(
            "alias-restore",
            &[
                "--input",
                "{{DIR}}/input.json",
                "--output",
                "{{URL}}/",
                "--type",
                "alias",
            ],
            false,
        )
    },
    // E16 legacy templates, E17 component + composable index templates
    Case {
        setup: Setup::LegacyTemplate,
        ..case(
            "template",
            &[
                "--input",
                "{{URL}}/golden_tpl",
                "--output",
                "{{OUT}}",
                "--type",
                "template",
            ],
            true,
        )
    },
    Case {
        setup: Setup::ComposableTemplates,
        ..case(
            "component-template",
            &[
                "--input",
                "{{URL}}/golden_comp*",
                "--output",
                "{{OUT}}",
                "--type",
                "component_template",
            ],
            true,
        )
    },
    Case {
        setup: Setup::ComposableTemplates,
        ..case(
            "index-template",
            &[
                "--input",
                "{{URL}}/golden_itpl",
                "--output",
                "{{OUT}}",
                "--type",
                "index_template",
            ],
            true,
        )
    },
    Case {
        setup: Setup::ComposableTemplates,
        prep: Some(&[
            "--input",
            "{{URL}}/golden_itpl",
            "--output",
            "{{DIR}}/input.json",
            "--type",
            "index_template",
        ]),
        ..case(
            "index-template-restore",
            &[
                "--input",
                "{{DIR}}/input.json",
                "--output",
                "{{URL}}/",
                "--type",
                "index_template",
            ],
            false,
        )
    },
    // E18 ILM (Elasticsearch default distribution) / E19 ISM (OpenSearch); ES 7.10 OSS has neither
    Case {
        setup: Setup::Policy,
        must_pass: false,
        ..case(
            "policy",
            &[
                "--input",
                "{{URL}}/golden_pol*",
                "--output",
                "{{OUT}}",
                "--type",
                "policy",
            ],
            true,
        )
    },
    // E20 stored scripts
    Case {
        setup: Setup::Script,
        ..case(
            "script",
            &[
                "--input",
                "{{URL}}/golden_scr*",
                "--output",
                "{{OUT}}",
                "--type",
                "script",
            ],
            true,
        )
    },
];

async fn setup(c: &Cluster, s: Setup) {
    match s {
        Setup::None => {}
        Setup::FreshDst => c.delete_index(DST).await,
        Setup::ShapedDst => {
            c.delete_index(DST).await;
            c.copy_shape(SRC, DST).await;
        }
        Setup::Alias => {
            c.call(
                "POST",
                "_aliases",
                Some(json!({"actions": [{"remove": {"index": SRC, "alias": "golden_alias"}}]})),
            )
            .await;
            c.ok(
                "POST",
                "_aliases",
                Some(json!({"actions": [{"add": {"index": SRC, "alias": "golden_alias", "filter": {"term": {"k": "key1"}}, "routing": "r1"}}]})),
            )
            .await;
        }
        Setup::LegacyTemplate => {
            let key = if c.major().await >= 7 || c.engine == Engine::OpenSearch {
                "index_patterns"
            } else {
                "template"
            };
            c.ok(
                "PUT",
                "_template/golden_tpl",
                Some(
                    json!({key: ["golden-tpl-*"], "order": 3, "settings": {"number_of_shards": 1}}),
                ),
            )
            .await;
        }
        Setup::ComposableTemplates => {
            c.call("DELETE", "_index_template/golden_itpl", None).await;
            c.call("DELETE", "_component_template/golden_comp", None)
                .await;
            c.ok(
                "PUT",
                "_component_template/golden_comp",
                Some(json!({"template": {"settings": {"number_of_shards": 1}}})),
            )
            .await;
            c.ok(
                "PUT",
                "_index_template/golden_itpl",
                Some(json!({"index_patterns": ["golden-itpl-*"], "composed_of": ["golden_comp"], "priority": 7})),
            )
            .await;
        }
        Setup::Policy => match c.engine {
            Engine::OpenSearch => {
                c.call("DELETE", "_plugins/_ism/policies/golden_pol", None)
                    .await;
                c.ok(
                    "PUT",
                    "_plugins/_ism/policies/golden_pol",
                    Some(
                        json!({"policy": {"description": "golden", "default_state": "hot",
                        "states": [{"name": "hot", "actions": [], "transitions": []}]}}),
                    ),
                )
                .await;
            }
            Engine::Elasticsearch if !c.is_oss().await => {
                c.call("DELETE", "_ilm/policy/golden_pol", None).await;
                c.ok(
                    "PUT",
                    "_ilm/policy/golden_pol",
                    Some(json!({"policy": {"phases": {"hot": {"actions": {"rollover": {"max_docs": 1000}}}}}})),
                )
                .await;
            }
            Engine::Elasticsearch => {}
        },
        Setup::Script => {
            c.ok(
                "PUT",
                "_scripts/golden_scr",
                Some(json!({"script": {"lang": "painless", "source": "ctx._source.n += 1"}})),
            )
            .await;
        }
    }
}

async fn cleanup(c: &Cluster) {
    for p in [
        "_index_template/golden_itpl",
        "_component_template/golden_comp",
        "_template/golden_tpl",
        "_scripts/golden_scr",
        "_plugins/_ism/policies/golden_pol",
        "_ilm/policy/golden_pol",
    ] {
        c.call("DELETE", p, None).await;
    }
    c.delete_index(SRC).await;
    c.delete_index(DST).await;
}

/// TST-API-036: the version's built-in templates and policies (name -> `_meta.managed`), so
/// that a new built-in in a future version shows up as drift and `--filterManaged` can be
/// checked against each version's own list. Built-ins are installed asynchronously after
/// startup: read until two reads 3 s apart agree (at most ~45 s).
async fn builtins(c: &Cluster) -> serde_json::Value {
    async fn snap(c: &Cluster) -> serde_json::Value {
        let managed = |b: &serde_json::Value| {
            b.pointer("/_meta/managed")
                .and_then(|m| m.as_bool())
                .unwrap_or(false)
        };
        let mut out = serde_json::Map::new();
        for (key, path, list) in [
            (
                "index_templates",
                "_index_template",
                Some("index_templates"),
            ),
            (
                "component_templates",
                "_component_template",
                Some("component_templates"),
            ),
            ("legacy_templates", "_template", None),
            ("ilm_policies", "_ilm/policy", None),
        ] {
            let (st, v) = c.call("GET", path, None).await;
            if st != 200 {
                continue;
            }
            let mut m = std::collections::BTreeMap::new();
            match list {
                Some(l) => {
                    for it in v[l].as_array().into_iter().flatten() {
                        let name = it["name"].as_str().unwrap_or("").to_string();
                        let body = it
                            .as_object()
                            .and_then(|o| {
                                o.iter().find(|(k, _)| *k != "name").map(|(_, b)| b.clone())
                            })
                            .unwrap_or_default();
                        m.insert(name, managed(&body));
                    }
                }
                None => {
                    for (name, body) in v.as_object().into_iter().flatten() {
                        let b = body.get("policy").unwrap_or(body);
                        m.insert(name.clone(), managed(b));
                    }
                }
            }
            m.retain(|k, _| !k.starts_with("golden") && !k.starts_with("it_"));
            out.insert(key.into(), serde_json::to_value(m).unwrap());
        }
        serde_json::Value::Object(out)
    }
    let mut prev = snap(c).await;
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let next = snap(c).await;
        if next == prev {
            return next;
        }
        prev = next;
    }
    prev
}

#[tokio::test]
#[ignore = "recording only: scripts/golden/record.sh (IT_RECORD=1)"]
async fn record() {
    if env("IT_RECORD").as_deref() != Some("1") {
        eprintln!("SKIP: golden::record needs IT_RECORD=1");
        return;
    }
    let c = Cluster::from_env();
    let version = c.version().await;
    let engine = match c.engine {
        Engine::OpenSearch => "opensearch",
        Engine::Elasticsearch if c.is_oss().await => "elasticsearch-oss",
        Engine::Elasticsearch => "elasticsearch",
    };
    let root = golden_root().join(format!("{engine}-{version}"));
    let only = env("GOLDEN_CASE");
    cleanup(&c).await;
    if only.is_none() {
        std::fs::create_dir_all(&root).unwrap();
        let b = builtins(&c).await;
        std::fs::write(
            root.join("builtins.json"),
            serde_json::to_string_pretty(&b).unwrap() + "\n",
        )
        .unwrap();
    }
    seed(&c).await;
    for case in CASES {
        if only.as_deref().is_some_and(|o| o != case.name) {
            continue;
        }
        let dir = root.join(case.name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if case.args.contains(&"{{DIR}}/input.ndjson") {
            std::fs::write(dir.join("input.ndjson"), input_for(case.name)).unwrap();
        }
        setup(&c, case.setup).await;
        let out = tmp_path(&format!("golden-{}", case.name), "out");
        if let Some(prep) = case.prep {
            let mut a: Vec<String> = prep
                .iter()
                .map(|x| {
                    x.replace("{{URL}}", c.url_with_auth("").trim_end_matches('/'))
                        .replace("{{DIR}}", dir.to_str().unwrap())
                })
                .collect();
            a.extend(c.tls_args("input"));
            assert_success(&run(a).await);
        }
        let rec = Recorder::start(&c.url_with_auth("")).await;
        let meta = Meta {
            case: case.name.into(),
            engine: engine.into(),
            version: version.clone(),
            image: env("IT_IMAGE").unwrap_or_default(),
            args: case.args.iter().map(|s| s.to_string()).collect(),
            exit_code: 0,
            has_output: case.output,
        };
        let o = run(meta.expand(&rec.url(), &out, &dir)).await;
        if case.must_pass {
            assert_success(&o);
        }
        let code = o.status.code().unwrap_or(-1);
        let n = rec.save(&dir).unwrap();
        let meta = Meta {
            exit_code: code,
            has_output: case.output && code == 0,
            ..meta
        };
        meta.write(&dir).unwrap();
        // expected.out comes from replaying the recording, so volatile values in the output
        // (e.g. ISM last_updated_time) are the normalised ones; the recording must replay cleanly.
        let rp = Replay::start(&dir).await;
        let rout = tmp_path(&format!("golden-{}-replay", case.name), "out");
        let ro = run(meta.expand(&rp.url(), &rout, &dir)).await;
        assert_eq!(
            ro.status.code(),
            Some(code),
            "{}: replay exit differs: {}",
            case.name,
            String::from_utf8_lossy(&ro.stderr)
        );
        assert!(
            rp.problems().is_empty(),
            "{}: recording does not replay: {:#?}",
            case.name,
            rp.problems()
        );
        if meta.has_output {
            let live = std::fs::read(&out).unwrap_or_default();
            let replayed = std::fs::read(&rout).unwrap_or_default();
            if live != replayed {
                eprintln!(
                    "golden: {}: live output differs from replay only in normalised values",
                    case.name
                );
            }
            std::fs::write(dir.join("expected.out"), replayed).unwrap();
        }
        let _ = std::fs::remove_file(&out);
        let _ = std::fs::remove_file(&rout);
        eprintln!(
            "golden: {}/{} recorded ({n} exchanges, exit {code})",
            root.display(),
            case.name
        );
    }
    cleanup(&c).await;
}
