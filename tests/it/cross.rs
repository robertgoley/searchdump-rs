//! Cross-engine copies (OpenSearch <-> Elasticsearch).
//!
//! * Cluster to cluster needs both engines up at once: only the `cross`
//!   profile (`IT_ES_URL` = OpenSearch, `IT_PEER_URL` = Elasticsearch) runs it,
//!   and `run.sh` skips that profile when the box has too little free memory.
//! * File dump/restore runs one container at a time: every plain profile
//!   exports a dump plus the expected result to `$IT_CROSS_DIR/<engine>/`
//!   and restores whatever the *other* engine exported earlier
//!   (`run.sh all` brings OpenSearch back once at the end for the last leg).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::harness::*;

fn pair() -> Option<(Cluster, Cluster)> {
    let peer = Cluster::peer()?;
    Some((Cluster::from_env(), peer))
}

fn both_directions() -> Option<Vec<(Cluster, Cluster)>> {
    let (a, b) = pair()?;
    Some(vec![(a.clone(), b.clone()), (b, a)])
}

fn name(c: &Cluster) -> &'static str {
    match c.engine {
        Engine::OpenSearch => "opensearch",
        Engine::Elasticsearch => "elasticsearch",
    }
}

async fn shape(c: &Cluster, index: &str) -> (Value, Value) {
    let st = c.ok("GET", &format!("{index}/_settings"), None).await;
    let i = &st[index]["settings"]["index"];
    let m = c.ok("GET", &format!("{index}/_mapping"), None).await;
    (
        json!({"shards": i["number_of_shards"], "replicas": i["number_of_replicas"], "analysis": i["analysis"]}),
        m[index]["mappings"].clone(),
    )
}

async fn green(c: &Cluster, index: &str) {
    c.ok(
        "GET",
        &format!("_cluster/health/{index}?wait_for_status=green&timeout=20s"),
        None,
    )
    .await;
}

#[tokio::test]
async fn cluster_to_cluster_full_copy_both_directions() {
    let Some(dirs) = both_directions() else {
        eprintln!("SKIP: no peer cluster (cross profile only)");
        return;
    };
    for (src_c, dst_c) in dirs {
        let label = format!("{}->{}", name(&src_c), name(&dst_c));
        let src = unique("x_src");
        let dst = unique("x_dst");
        seed_index(&src_c, &src, 1200).await;
        dst_c.delete_index(&dst).await;
        for ty in ["settings", "mapping", "data"] {
            let o = run_loud(s(&[
                "--input",
                &src_c.url_with_auth(&src),
                "--output",
                &dst_c.url_with_auth(&dst),
                "--type",
                ty,
                "--limit",
                "500",
            ]))
            .await;
            assert_success(&o);
            let err = String::from_utf8_lossy(&o.stderr);
            // the source engine's own keys are dropped with a warning (OpenSearch 1.x has no
            // index.replication.type, so nothing is dropped there)
            let own = src_c
                .ok("GET", &format!("{src}/_settings?flat_settings=true"), None)
                .await[&src]["settings"]
                .get(own_key(src_c.engine))
                .is_some();
            if ty == "settings" && own {
                let dropped = match src_c.engine {
                    Engine::Elasticsearch => "_tier_preference",
                    Engine::OpenSearch => "replication.type",
                };
                assert!(
                    err.contains("Dropping") && err.contains(dropped),
                    "{label}: {err}"
                );
            }
        }
        green(&dst_c, &dst).await;
        assert_eq!(
            shape(&src_c, &src).await,
            shape(&dst_c, &dst).await,
            "{label}"
        );
        assert_eq!(src_c.docs(&src).await, dst_c.docs(&dst).await, "{label}");
        src_c.delete_index(&src).await;
        dst_c.delete_index(&dst).await;
    }
}

type Docs = BTreeMap<String, (Option<String>, Value)>;

const TYPES: [&str; 3] = ["settings", "mapping", "data"];

fn cross_base() -> Option<PathBuf> {
    // plain profiles only: the secured ones exercise the same code paths
    if is_secure() {
        return None;
    }
    std::env::var("IT_CROSS_DIR").ok().map(PathBuf::from)
}

/// Exports of one engine, one directory per major version
/// (`elasticsearch-8`, `elasticsearch-9`, `opensearch-2`, ...).
fn exports_of(e: Engine) -> Vec<PathBuf> {
    let Some(base) = cross_base() else {
        return Vec::new();
    };
    let prefix = format!("{}-", engine_name(e));
    let mut v: Vec<PathBuf> = std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|d| d.file_name().to_string_lossy().starts_with(&prefix))
        .map(|d| d.path())
        .filter(|p| p.join("expected.json").exists())
        .collect();
    v.sort();
    v
}

fn engine_name(e: Engine) -> &'static str {
    match e {
        Engine::OpenSearch => "opensearch",
        Engine::Elasticsearch => "elasticsearch",
    }
}

fn other(e: Engine) -> Engine {
    match e {
        Engine::OpenSearch => Engine::Elasticsearch,
        Engine::Elasticsearch => Engine::OpenSearch,
    }
}

/// Settings key the given engine adds by itself and the other one lacks.
fn own_key(e: Engine) -> &'static str {
    match e {
        Engine::Elasticsearch => "index.routing.allocation.include._tier_preference",
        Engine::OpenSearch => "index.replication.type",
    }
}

#[tokio::test]
async fn file_export_for_the_other_engine() {
    let c = Cluster::from_env();
    let Some(base) = cross_base() else {
        eprintln!("SKIP: IT_CROSS_DIR not set or secured profile");
        return;
    };
    let dir = base.join(format!("{}-{}", engine_name(c.engine), c.version().await));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = unique("xf_src");
    seed_index(&c, &src, 800).await;
    for ty in TYPES {
        let f = dir.join(format!("{ty}.json.zst"));
        assert_success(
            &run(s(&[
                "--input",
                &c.url_with_auth(&src),
                "--output",
                f.to_str().unwrap(),
                "--type",
                ty,
                "--compressionType",
                "zstd",
            ]))
            .await,
        );
    }
    let (settings, mapping) = shape(&c, &src).await;
    // ES OSS builds have no tiers and OpenSearch 1.x no replication.type
    let live = c
        .ok("GET", &format!("{src}/_settings?flat_settings=true"), None)
        .await;
    let has_own_key = live[&src]["settings"].get(own_key(c.engine)).is_some();
    let policy = export_policy(&c, &dir).await;
    let expected = json!({
        "has_own_key": has_own_key,
        "policy": policy,
        "version": c.version().await,
        "settings": settings,
        "mapping": mapping,
        "docs": c.docs(&src).await,
    });
    std::fs::write(dir.join("expected.json"), expected.to_string()).unwrap();
    c.delete_index(&src).await;
}

#[tokio::test]
async fn file_dump_from_the_other_engine_restores_here() {
    let c = Cluster::from_env();
    let from = other(c.engine);
    let dirs = exports_of(from);
    if dirs.is_empty() {
        eprintln!(
            "SKIP: no {} export yet (or IT_CROSS_DIR unset)",
            engine_name(from)
        );
        return;
    }
    for dir in dirs {
        restore_export(&c, from, &dir).await;
    }
}

async fn restore_export(c: &Cluster, from: Engine, dir: &std::path::Path) {
    let raw = std::fs::read_to_string(dir.join("expected.json")).unwrap();
    let label = format!(
        "{}->{}",
        dir.file_name().unwrap().to_string_lossy(),
        engine_name(c.engine)
    );
    let expected: Value = serde_json::from_str(&raw).unwrap();
    eprintln!("{label}: dump written by version {}", expected["version"]);
    let settings_file = dir.join("settings.json.zst");
    let foreign = own_key(from);

    // 1. without --excludeSettings the foreign key is reported: OpenSearch keys
    //    are rejected by Elasticsearch, Elasticsearch keys are stored silently
    //    by OpenSearch, so the tool warns before sending them
    let probe = unique("xf_probe");
    c.delete_index(&probe).await;
    let o = run_loud(s(&[
        "--input",
        settings_file.to_str().unwrap(),
        "--output",
        &c.url_with_auth(&probe),
        "--type",
        "settings",
    ]))
    .await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    let short = foreign.rsplit('.').next().unwrap();
    if expected["has_own_key"] != false {
        assert!(
            err.contains("--excludeSettings") && err.contains(short),
            "{label}: {err}"
        );
        match c.engine {
            Engine::Elasticsearch => assert!(!o.status.success(), "{label}: {err}"),
            Engine::OpenSearch => {
                assert_success(&o);
                assert!(err.contains("Elasticsearch-only"), "{label}: {err}");
            }
        }
    } else {
        eprintln!("{label}: the source had no {foreign}; nothing to warn about");
        assert_success(&o);
    }
    c.delete_index(&probe).await;
    if let Some(name) = expected["policy"].as_str() {
        restore_policy_export(c, &label, &dir.join("policy.json"), name).await;
    }

    // 2. full restore with the foreign key excluded
    let dst = unique("xf_dst");
    c.delete_index(&dst).await;
    for ty in TYPES {
        let f = dir.join(format!("{ty}.json.zst"));
        let mut a = s(&[
            "--input",
            f.to_str().unwrap(),
            "--output",
            &c.url_with_auth(&dst),
            "--type",
            ty,
        ]);
        if ty == "settings" {
            a.extend(s(&["--excludeSettings", foreign]));
        }
        let o = run_loud(a).await;
        assert_success(&o);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!err.contains("-only keys"), "{label}: {err}");
    }
    green(c, &dst).await;
    let (settings, mapping) = shape(c, &dst).await;
    assert_eq!(settings, expected["settings"], "{label}");
    assert_eq!(mapping, expected["mapping"], "{label}");
    let want: Docs = serde_json::from_value(expected["docs"].clone()).unwrap();
    assert_eq!(c.docs(&dst).await, want, "{label}");
    c.delete_index(&dst).await;
}

// ------------------------------------------------- policies across engines

/// An ILM policy using every convertible action, or an ISM policy.
fn ilm_policy() -> Value {
    // max_size (not max_primary_shard_size): ES 7.10 has no shard-size rollover
    json!({"policy": {
        "_meta": {"description": "cross-engine conversion"},
        "phases": {
            "hot": {"actions": {"rollover": {"max_age": "1d", "max_size": "50gb"}, "set_priority": {"priority": 100}}},
            "warm": {"min_age": "7d", "actions": {"forcemerge": {"max_num_segments": 1}, "readonly": {}, "allocate": {"number_of_replicas": 0}, "set_priority": {"priority": 50}}},
            "delete": {"min_age": "30d", "actions": {"delete": {}}}
        }
    }})
}

fn ism_policy() -> Value {
    json!({"policy": {"description": "cross-engine", "default_state": "hot",
        "states": [{"name": "hot", "actions": [], "transitions": [{"state_name": "delete", "conditions": {"min_index_age": "30d"}}]},
                   {"name": "delete", "actions": [{"delete": {}}], "transitions": []}]}})
}

fn policy_path(e: Engine, name: &str) -> String {
    match e {
        Engine::Elasticsearch => format!("_ilm/policy/{name}"),
        Engine::OpenSearch => format!("_plugins/_ism/policies/{name}"),
    }
}

/// Dump one policy of this cluster to `dir/policy.json`; `None` when the
/// cluster has no policy API (Elasticsearch OSS build).
async fn export_policy(c: &Cluster, dir: &std::path::Path) -> Option<String> {
    if !c.has_policies().await {
        return None;
    }
    let name = unique("xf_pol");
    let body = match c.engine {
        Engine::Elasticsearch => ilm_policy(),
        Engine::OpenSearch => ism_policy(),
    };
    c.ok("PUT", &policy_path(c.engine, &name), Some(body)).await;
    let f = dir.join("policy.json");
    assert_success(
        &run(s(&[
            "--input",
            &c.url_with_auth(&name),
            "--output",
            f.to_str().unwrap(),
            "--type",
            "policy",
        ]))
        .await,
    );
    c.ok("DELETE", &policy_path(c.engine, &name), None).await;
    Some(name)
}

async fn restore_policy_export(c: &Cluster, label: &str, file: &std::path::Path, name: &str) {
    let f = file.to_str().unwrap();
    let out = c.url_with_auth("");
    let plain = run_loud(s(&["--input", f, "--output", &out, "--type", "policy"])).await;
    let err = String::from_utf8_lossy(&plain.stderr).to_string();
    match c.engine {
        Engine::Elasticsearch => {
            // ISM -> ILM: always refused, with or without the flag. An OSS build (ES 7.10 OSS) has
            // no ILM at all, so the tool refuses earlier with the OSS message instead.
            let oss = c.is_oss().await;
            let refused = |e: &str| {
                if oss {
                    e.contains("ILM is an X-Pack feature") && e.contains("OSS build")
                } else {
                    e.contains("ISM → ILM")
                }
            };
            assert!(!plain.status.success() && refused(&err), "{label}: {err}");
            let o = run_loud(s(&[
                "--input",
                f,
                "--output",
                &out,
                "--type",
                "policy",
                "--convertIlmToIsm",
            ]))
            .await;
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            assert!(!o.status.success(), "{label}: {err}");
            assert!(!oss || refused(&err), "{label}: {err}");
            if !oss {
                // An OSS build has no _ilm endpoint to ask; the refusal above is the check there.
                let (st, _) = c.call("GET", &policy_path(c.engine, name), None).await;
                assert_eq!(st, 404, "{label}");
            }
        }
        Engine::OpenSearch => {
            // ILM -> ISM: refused without the flag, converted with it
            assert!(
                !plain.status.success() && err.contains("--convertIlmToIsm"),
                "{label}: {err}"
            );
            let o = run_loud(s(&[
                "--input",
                f,
                "--output",
                &out,
                "--type",
                "policy",
                "--convertIlmToIsm",
            ]))
            .await;
            let err = String::from_utf8_lossy(&o.stderr).to_string();
            assert_success(&o);
            assert!(
                err.contains("rollover_alias") && err.contains("not attached"),
                "{label}: {err}"
            );
            let g = c.ok("GET", &policy_path(c.engine, name), None).await;
            let states: Vec<&str> = g["policy"]["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["name"].as_str().unwrap())
                .collect();
            assert_eq!(states, ["hot", "warm", "delete"], "{label}: {g}");
            assert_eq!(g["policy"]["default_state"], "hot");
            assert_eq!(g["policy"]["description"], "cross-engine conversion");
            c.ok("DELETE", &policy_path(c.engine, name), None).await;
            unsafe_conversion_is_refused_unless_lenient(c, label).await;
        }
    }
}

async fn unsafe_conversion_is_refused_unless_lenient(c: &Cluster, label: &str) {
    let name = unique("xf_unsafe");
    let doc = json!({ name.as_str(): {"version": 1, "policy": {"phases": {
        "hot": {"actions": {"rollover": {"max_age": "1d"}}},
        "cold": {"min_age": "30d", "actions": {"searchable_snapshot": {"snapshot_repository": "repo"}}}}}}});
    let f = tmp_path("unsafe_ilm", "json");
    std::fs::write(
        &f,
        serde_json::to_string(&Value::String(doc.to_string())).unwrap(),
    )
    .unwrap();
    let out = c.url_with_auth("");
    let base = s(&[
        "--input",
        f.to_str().unwrap(),
        "--output",
        &out,
        "--type",
        "policy",
        "--convertIlmToIsm",
    ]);
    let o = run_loud(base.clone()).await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(
        !o.status.success()
            && err.contains("searchable_snapshot")
            && err.contains("--lenientPolicyConversion"),
        "{label}: {err}"
    );
    let mut a = base;
    a.push("--lenientPolicyConversion".into());
    let o = run_loud(a).await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert_success(&o);
    assert!(
        err.contains("DROPPED") && err.contains("searchable_snapshot"),
        "{label}: {err}"
    );
    c.ok("DELETE", &policy_path(c.engine, &name), None).await;
}

#[tokio::test]
async fn cluster_to_cluster_policy_copy_converts_only_on_request() {
    let Some((os, es)) = pair() else {
        eprintln!("SKIP: no peer cluster (cross profile only)");
        return;
    };
    let (os, es) = if os.engine == Engine::OpenSearch {
        (os, es)
    } else {
        (es, os)
    };
    let name = unique("xc_pol");
    es.ok("PUT", &policy_path(es.engine, &name), Some(ilm_policy()))
        .await;
    let args = |extra: &[&str]| {
        let mut a = s(&[
            "--input",
            &es.url_with_auth(&name),
            "--output",
            &os.url_with_auth(""),
            "--type",
            "policy",
        ]);
        a.extend(s(extra));
        a
    };
    let o = run_loud(args(&[])).await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(
        !o.status.success() && err.contains("--convertIlmToIsm"),
        "{err}"
    );
    assert_eq!(
        os.call("GET", &policy_path(os.engine, &name), None).await.0,
        404
    );
    let o = run_loud(args(&["--convertIlmToIsm"])).await;
    assert_success(&o);
    let g = os.ok("GET", &policy_path(os.engine, &name), None).await;
    assert_eq!(g["policy"]["states"].as_array().unwrap().len(), 3, "{g}");

    // OpenSearch -> Elasticsearch: refused
    let o = run_loud(s(&[
        "--input",
        &os.url_with_auth(&name),
        "--output",
        &es.url_with_auth(""),
        "--type",
        "policy",
        "--convertIlmToIsm",
    ]))
    .await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!o.status.success() && err.contains("ISM → ILM"), "{err}");

    // templates copy ES -> OS: Elasticsearch-only fields stripped with a warning
    let t = unique("xc_tpl");
    es.ok("PUT", &format!("_index_template/{t}"), Some(json!({"index_patterns": [format!("{t}-*")], "priority": 917, "allow_auto_create": true, "template": {"settings": {"index": {"number_of_replicas": "0"}}}}))).await;
    let o = run_loud(s(&[
        "--input",
        &es.url_with_auth(&t),
        "--output",
        &os.url_with_auth(""),
        "--type",
        "index_template",
    ]))
    .await;
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert_success(&o);
    assert!(err.contains("allow_auto_create"), "{err}");
    os.ok("GET", &format!("_index_template/{t}"), None).await;
    es.ok("DELETE", &format!("_index_template/{t}"), None).await;
    os.ok("DELETE", &format!("_index_template/{t}"), None).await;
    es.ok("DELETE", &policy_path(es.engine, &name), None).await;
    os.ok("DELETE", &policy_path(os.engine, &name), None).await;
}
