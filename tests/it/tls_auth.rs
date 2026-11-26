//! M1: TLS and authentication against the secured profiles (OpenSearch demo
//! security, or bitnami Elasticsearch with basic auth + TLS). Every test is a
//! no-op on the plain profiles.

use crate::harness::*;

struct Secure {
    c: Cluster,
    ca: String,
    cert: String,
    key: String,
    key_enc: String,
    key_pass: String,
    user: String,
    pw: String,
}

fn secure() -> Option<Secure> {
    if !is_secure() {
        eprintln!("skipping: not a secured profile");
        return None;
    }
    Some(Secure {
        c: Cluster::from_env(),
        ca: env("IT_CA")?,
        cert: env("IT_CLIENT_CERT")?,
        key: env("IT_CLIENT_KEY")?,
        key_enc: env("IT_CLIENT_KEY_ENC")?,
        key_pass: env("IT_CLIENT_KEY_PASS")?,
        user: env("IT_ADMIN_USER")?,
        pw: env("IT_ADMIN_PW")?,
    })
}

async fn seeded(s: &Secure, label: &str) -> String {
    let index = unique(label);
    seed_index(&s.c, &index, 50).await;
    index
}

async fn dump(
    args: Vec<String>,
    envs: Vec<(String, String)>,
) -> (std::process::Output, std::path::PathBuf) {
    let out = tmp_path("tls", "json");
    let mut a = args;
    a.extend(s(&["--output", out.to_str().unwrap()]));
    (run_env(a, envs).await, out)
}

async fn expect_full_dump(s: &Secure, index: &str, args: Vec<String>, envs: Vec<(String, String)>) {
    let (o, out) = dump(args, envs).await;
    assert_success(&o);
    assert_eq!(lines(&out).len() as u64, s.c.count(index).await);
    let _ = std::fs::remove_file(out);
}

#[tokio::test]
async fn input_ca_with_url_credentials() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_ca").await;
    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index), "--input-ca", &s.ca]),
        vec![],
    )
    .await;
    // shared --ca and legacy --caFile work too
    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index), "--ca", &s.ca]),
        vec![],
    )
    .await;
    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index), "--caFile", &s.ca]),
        vec![],
    )
    .await;
    s.c.delete_index(&index).await;
}

#[tokio::test]
async fn verification_fails_without_ca_and_insecure_skips_it() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_insecure").await;
    let (o, _) = dump(
        s_args(&["--input", &s.c.url_with_auth(&index), "--noImplicitRetry"]),
        vec![],
    )
    .await;
    assert!(
        !o.status.success(),
        "self-signed cert must be rejected without a CA"
    );
    let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
    assert!(
        err.contains("certificate") || err.contains("unknownissuer"),
        "{err}"
    );

    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index), "--insecure"]),
        vec![],
    )
    .await;
    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index), "--input-insecure"]),
        vec![],
    )
    .await;
    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &s.c.url_with_auth(&index)]),
        vec![("NODE_TLS_REJECT_UNAUTHORIZED".into(), "0".into())],
    )
    .await;
    s.c.delete_index(&index).await;
}

#[tokio::test]
async fn client_certificate_plain_and_encrypted_key() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_mtls").await;
    // OpenSearch: the admin (kirk) certificate authenticates on its own.
    // Elasticsearch: certificates are verified (client_authentication:
    // optional) but authentication stays basic auth.
    let url = if cert_auth() {
        s.c.index_url(&index)
    } else {
        s.c.url_with_auth(&index)
    };
    if !cert_auth() {
        // prove the client certificate is really presented: one from an
        // unknown CA fails the handshake
        let d = tmp_path("rogue", "d");
        std::fs::create_dir_all(&d).unwrap();
        let (k, c) = (d.join("k.pem"), d.join("c.pem"));
        let st = std::process::Command::new("openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
            ])
            .args(["-subj", "/CN=rogue", "-keyout"])
            .arg(&k)
            .arg("-out")
            .arg(&c)
            .output()
            .unwrap();
        assert!(st.status.success());
        let (o, _) = dump(
            s_args(&[
                "--input",
                &url,
                "--input-ca",
                &s.ca,
                "--input-cert",
                c.to_str().unwrap(),
                "--input-key",
                k.to_str().unwrap(),
                "--noImplicitRetry",
            ]),
            vec![],
        )
        .await;
        assert!(
            !o.status.success(),
            "untrusted client cert must be rejected"
        );
    }
    expect_full_dump(
        &s,
        &index,
        s_args(&[
            "--input",
            &url,
            "--input-ca",
            &s.ca,
            "--input-cert",
            &s.cert,
            "--input-key",
            &s.key,
            "--tlsAuth",
        ]),
        vec![],
    )
    .await;
    expect_full_dump(
        &s,
        &index,
        s_args(&[
            "--input",
            &url,
            "--input-ca",
            &s.ca,
            "--input-cert",
            &s.cert,
            "--input-key",
            &s.key_enc,
            "--input-pass",
            &s.key_pass,
            "--tlsAuth",
        ]),
        vec![],
    )
    .await;
    // wrong passphrase is a clear error
    let (o, _) = dump(
        s_args(&[
            "--input",
            &url,
            "--input-ca",
            &s.ca,
            "--input-cert",
            &s.cert,
            "--input-key",
            &s.key_enc,
            "--input-pass",
            "wrong",
            "--noImplicitRetry",
        ]),
        vec![],
    )
    .await;
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
    assert!(
        err.contains("decrypt") || err.contains("passphrase"),
        "{err}"
    );
    s.c.delete_index(&index).await;
}

#[tokio::test]
async fn credentials_from_env_and_auth_file() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_creds").await;
    let url = s.c.index_url(&index);

    // no credentials at all → 401
    let (o, _) = dump(
        s_args(&["--input", &url, "--input-ca", &s.ca, "--noImplicitRetry"]),
        vec![],
    )
    .await;
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("401"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );

    expect_full_dump(
        &s,
        &index,
        s_args(&["--input", &url, "--input-ca", &s.ca]),
        vec![
            ("SEARCHDUMP_INPUT_USERNAME".into(), s.user.clone()),
            ("SEARCHDUMP_INPUT_PASSWORD".into(), s.pw.clone()),
        ],
    )
    .await;

    let auth = tmp_path("auth", "ini");
    std::fs::write(&auth, format!("user={}\npassword={}\n", s.user, s.pw)).unwrap();
    expect_full_dump(
        &s,
        &index,
        s_args(&[
            "--input",
            &url,
            "--input-ca",
            &s.ca,
            "--httpAuthFile",
            auth.to_str().unwrap(),
        ]),
        vec![],
    )
    .await;
    let _ = std::fs::remove_file(auth);
    s.c.delete_index(&index).await;
}

fn s_args(a: &[&str]) -> Vec<String> {
    s(a)
}

// ------------------------------------------------------------------ L4-001 / L4-002: multi

fn multi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_searchdump-multi")
}

/// Run searchdump-multi with exactly these extra variables (the credential and passphrase
/// variables of the test process are removed first).
async fn multi(args: Vec<String>, envs: Vec<(String, String)>) -> std::process::Output {
    tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(multi_bin());
        cmd.args(&args).env("SEARCHDUMP_BIN", bin());
        for k in [
            "SEARCHDUMP_INPUT_USERNAME",
            "SEARCHDUMP_INPUT_PASSWORD",
            "SEARCHDUMP_OUTPUT_USERNAME",
            "SEARCHDUMP_OUTPUT_PASSWORD",
            "SEARCHDUMP_TLS_PASS",
            "SEARCHDUMP_INPUT_TLS_PASS",
            "SEARCHDUMP_OUTPUT_TLS_PASS",
            "NODE_TLS_REJECT_UNAUTHORIZED",
        ] {
            cmd.env_remove(k);
        }
        cmd.envs(envs).output().expect("spawn searchdump-multi")
    })
    .await
    .unwrap()
}

fn all_text(dir: &std::path::Path) -> String {
    let mut t = String::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_file() {
            t.push_str(&String::from_utf8_lossy(
                &std::fs::read(&p).unwrap_or_default(),
            ));
        }
    }
    t
}

/// L4-001 (1.4, C11): searchdump-multi reaches a cluster behind a private CA with a client
/// certificate whose key is encrypted, the passphrase only in `$SEARCHDUMP_TLS_PASS`: the parent
/// and every child verify the server with `--ca` and present the certificate, a dump and a load
/// round-trip, and the passphrase is in no output, log, manifest or child report. L4-002: every
/// child run has an id in the parent's lines, its own log lines, the manifest and children.json.
#[tokio::test]
async fn multi_private_ca_client_cert_and_env_passphrase() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_multi").await;
    let dir = tmp_path("tls_multi", "d");
    let logs = tmp_path("tls_multi_logs", "d");
    let cert_flags = ["--ca", &s.ca, "--cert", &s.cert, "--key", &s.key_enc];
    // OpenSearch: the admin certificate authenticates on its own; Elasticsearch: basic auth
    // from the environment, the certificate is presented and verified
    let mut envs = vec![("SEARCHDUMP_TLS_PASS".to_string(), s.key_pass.clone())];
    if !cert_auth() {
        for d in ["INPUT", "OUTPUT"] {
            envs.push((format!("SEARCHDUMP_{d}_USERNAME"), s.user.clone()));
            envs.push((format!("SEARCHDUMP_{d}_PASSWORD"), s.pw.clone()));
        }
    }
    let mut a = s_args(&[
        "--direction",
        "dump",
        "--input",
        &s.c.url,
        "--output",
        dir.to_str().unwrap(),
        "--match",
        &format!("^{index}$"),
        "--logDir",
        logs.to_str().unwrap(),
    ]);
    a.extend(s_args(&cert_flags));
    let o = multi(a, envs.clone()).await;
    let (out, err) = (
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    );
    assert!(o.status.success(), "{out}\n{err}");
    assert!(out.contains(&format!("ok      {index}")), "{out}");
    // L4-002: ids in the parent's lines, the manifest and the child logs
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("multielasticdump.json")).unwrap())
            .unwrap();
    let children = manifest["indices"][&index]["children"]
        .as_array()
        .unwrap()
        .clone();
    assert!(children.len() >= 3, "{manifest}");
    for c in &children {
        let id = c["id"].as_str().unwrap();
        assert!(err.contains(&format!("child {id} start: {index}")), "{err}");
        assert!(err.contains(&format!("child {id} end: {index}")), "{err}");
        let step = c["step"].as_str().unwrap();
        let log = std::fs::read_to_string(logs.join(format!("{index}.{step}.log"))).unwrap();
        for l in log.lines().filter(|l| l.starts_with("[20")) {
            assert!(l.contains(&format!(" child {id}] ")), "{l}");
        }
    }
    let data_id = children.iter().find(|c| c["step"] == "data").unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let stats: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(logs.join(format!("{index}.data.stats.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(stats["child_id"], data_id.as_str());
    // the passphrase is nowhere
    for (what, text) in [
        ("stdout", out.clone()),
        ("stderr", err.clone()),
        ("logs", all_text(&logs)),
        ("dump", all_text(&dir)),
    ] {
        assert!(!text.contains(&s.key_pass), "passphrase in {what}");
    }

    // load it back under a prefix through the same TLS settings (children get --output-*)
    let mut a = s_args(&[
        "--direction",
        "load",
        "--input",
        dir.to_str().unwrap(),
        "--output",
        &s.c.url,
        "--prefix",
        "l4copy-",
        "--logDir",
        logs.to_str().unwrap(),
    ]);
    a.extend(s_args(&cert_flags));
    let o = multi(a, envs.clone()).await;
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{err}");
    let copy = format!("l4copy-{index}");
    assert_eq!(s.c.count(&copy).await, s.c.count(&index).await);
    let report: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(logs.join("children.json")).unwrap())
            .unwrap();
    let data = report
        .iter()
        .find(|r| {
            r["index"] == copy.as_str() || (r["index"] == index.as_str() && r["step"] == "data")
        })
        .expect("data child in the report");
    assert_eq!(data["status"], "ok");
    // the load child noted its target index uuid at the end
    assert!(data["target_index"]["uuid_final"].is_string(), "{data}");
    assert!(!all_text(&logs).contains(&s.key_pass));

    // without --ca the parent's own verification fails, before any child starts
    let a = s_args(&[
        "--direction",
        "dump",
        "--input",
        &s.c.url,
        "--output",
        dir.to_str().unwrap(),
        "--match",
        &format!("^{index}$"),
    ]);
    let o = multi(a, envs.clone()).await;
    assert!(!o.status.success());
    let err = String::from_utf8_lossy(&o.stderr).to_lowercase();
    assert!(
        err.contains("certificate") || err.contains("unknownissuer"),
        "{err}"
    );
    assert!(!err.contains("child "), "{err}");
    // a wrong passphrase fails without echoing it
    let mut bad = envs.clone();
    bad[0].1 = "FAKE-wrong-pass-l4".to_string();
    let mut a = s_args(&[
        "--direction",
        "dump",
        "--input",
        &s.c.url,
        "--output",
        dir.to_str().unwrap(),
        "--match",
        &format!("^{index}$"),
    ]);
    a.extend(s_args(&cert_flags));
    let o = multi(a, bad).await;
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(!o.status.success());
    assert!(err.contains("decrypt"), "{err}");
    assert!(!err.contains("FAKE-wrong-pass-l4"), "{err}");

    s.c.delete_index(&index).await;
    s.c.delete_index(&copy).await;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&logs);
}

/// t255u: searchdump-multi honours `NODE_TLS_REJECT_UNAUTHORIZED=0` like searchdump and Node
/// elasticdump: without `--ca` the private-CA cluster is reached by the parent and every child,
/// with searchdump's warnings; without the variable the same run fails verification.
#[tokio::test]
async fn multi_honours_node_tls_reject_unauthorized() {
    let Some(s) = secure() else { return };
    let index = seeded(&s, "tls_multi_node").await;
    let dir = tmp_path("tls_multi_node", "d");
    let logs = tmp_path("tls_multi_node_logs", "d");
    let mut envs: Vec<(String, String)> = vec![];
    let mut a = s_args(&[
        "--direction",
        "dump",
        "--input",
        &s.c.url,
        "--output",
        dir.to_str().unwrap(),
        "--match",
        &format!("^{index}$"),
        "--logDir",
        logs.to_str().unwrap(),
    ]);
    if cert_auth() {
        // the admin certificate authenticates (no CA given: only verification is off)
        envs.push(("SEARCHDUMP_TLS_PASS".into(), s.key_pass.clone()));
        a.extend(s_args(&["--cert", &s.cert, "--key", &s.key_enc]));
    } else {
        envs.push(("SEARCHDUMP_INPUT_USERNAME".into(), s.user.clone()));
        envs.push(("SEARCHDUMP_INPUT_PASSWORD".into(), s.pw.clone()));
    }
    let o = multi(a.clone(), envs.clone()).await;
    assert!(!o.status.success(), "verification on without the variable");
    let mut node = envs.clone();
    node.push(("NODE_TLS_REJECT_UNAUTHORIZED".into(), "0".into()));
    let o = multi(a, node).await;
    let err = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{err}");
    assert!(
        err.contains("NODE_TLS_REJECT_UNAUTHORIZED=0: TLS certificate verification is disabled"),
        "{err}"
    );
    assert!(
        err.contains("TLS certificate verification is DISABLED for"),
        "{err}"
    );
    // the data child ran without verification too, and said so in its log
    let log = std::fs::read_to_string(logs.join(format!("{index}.data.log"))).unwrap();
    assert!(log.contains("verification is DISABLED"), "{log}");
    let n = s.c.count(&index).await;
    let out = String::from_utf8_lossy(&o.stdout).into_owned();
    assert!(out.contains(&format!("({n} records)")), "{out}");
    s.c.delete_index(&index).await;
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&logs);
}
