//! Secret-handling regressions (TEST-REQUIREMENTS.md §7, F-05..F-08).

mod support;

use std::process::Output;

use support::mock_es::{CannedResponse, MockEs};

fn bin() -> std::process::Command {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_searchdump"));
    for k in [
        "SEARCHDUMP_INPUT_USERNAME",
        "SEARCHDUMP_INPUT_PASSWORD",
        "SEARCHDUMP_OUTPUT_USERNAME",
        "SEARCHDUMP_OUTPUT_PASSWORD",
        "RUST_LOG",
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "NO_PROXY",
        "no_proxy",
    ] {
        c.env_remove(k);
    }
    c
}

async fn run(args: Vec<String>, env: Vec<(&'static str, String)>) -> Output {
    tokio::task::spawn_blocking(move || bin().args(args).envs(env).output().expect("spawn"))
        .await
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn scroll() -> Vec<CannedResponse> {
    let page = |hits: &str| {
        format!(
            r#"{{"_scroll_id":"s1","timed_out":false,"_shards":{{"total":1,"successful":1,"skipped":0,"failed":0}},"hits":{{"total":{{"value":1,"relation":"eq"}},"hits":[{hits}]}}}}"#
        )
    };
    vec![
        CannedResponse::ok(page(r#"{"_index":"i","_id":"1","_source":{"n":1}}"#)),
        CannedResponse::ok(page("")),
        CannedResponse::ok(r#"{"succeeded":true,"num_freed":1}"#),
    ]
}

/// F-05: --debug logs requests without --params values or passwords.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn debug_log_masks_params_and_credentials() {
    let mock = MockEs::serve(scroll()).await;
    let url = mock
        .input_url("i")
        .replace("http://", "http://user:PW-SECRET@");
    let o = run(
        vec![
            "--input".into(),
            url,
            "--output".into(),
            "$".into(),
            "--debug".into(),
            "--input-params".into(),
            r#"{"api_key":"Q-SECRET"}"#.into(),
        ],
        vec![],
    )
    .await;
    assert!(o.status.success(), "{}", stderr(&o));
    let err = stderr(&o);
    assert!(err.contains("api_key=***"), "{err}");
    assert!(!err.contains("Q-SECRET"), "{err}");
    assert!(!err.contains("PW-SECRET"), "{err}");
    // the server still got the real value
    assert!(mock.requests()[0].contains("api_key=Q-SECRET"));
}

/// F-06: a scheme-less `user:pw@host` endpoint is a file path; it is masked
/// in every message and a warning says it is treated as a file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn schemeless_url_is_masked_and_warned() {
    let o = run(
        vec![
            "--input".into(),
            "elastic:PW-SECRET@localhost:9200/idx".into(),
            "--output".into(),
            "$".into(),
        ],
        vec![],
    )
    .await;
    let err = stderr(&o);
    assert!(!err.contains("PW-SECRET"), "{err}");
    assert!(err.contains("treated as a FILE path"), "{err}");
}

/// F-07: an environment proxy is announced; --noProxy connects directly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn env_proxy_is_visible_and_can_be_disabled() {
    let mock = MockEs::serve(scroll()).await;
    let proxy = vec![(
        "HTTP_PROXY",
        "http://pu:PROXY-SECRET@127.0.0.1:9".to_string(),
    )];
    let args = |extra: Option<&str>| {
        let mut v = vec![
            "--input".to_string(),
            mock.input_url("i"),
            "--output".into(),
            "$".into(),
            "--noImplicitRetry".into(),
        ];
        v.extend(extra.map(String::from));
        v
    };
    let o = run(args(None), proxy.clone()).await;
    let err = stderr(&o);
    assert!(err.contains("go through proxy"), "{err}");
    assert!(err.contains("HTTP_PROXY"), "{err}");
    assert!(!err.contains("PROXY-SECRET"), "{err}");
    let o = run(args(Some("--noProxy")), proxy).await;
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(!stderr(&o).contains("go through proxy"), "{}", stderr(&o));
}

/// L-453 (TST-BRK-CLI-007): canaries planted in every credential channel never reach
/// stdout, stderr, output files or file names, for every error class, with and
/// without --debug -- including a server that echoes the whole request back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn secret_canaries_never_leak() {
    use base64::Engine;
    use support::router_mock::{Recorded, Reply, RouterMock};
    let echo = |status: u16| {
        RouterMock::builder()
            .handler(move |r: Recorded| async move {
                let q: Vec<String> = r.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
                let h: Vec<String> = r.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                let echoed = format!("{} {}?{} | {}", r.method, r.path, q.join("&"), h.join(" | "));
                Reply::json(
                    status,
                    serde_json::json!({"error": {"type": "echo", "reason": echoed}, "status": status})
                        .to_string(),
                )
            })
            .start()
    };
    let m401 = echo(401).await;
    let m403 = echo(403).await;
    let m500 = echo(500).await;
    let dir = tempfile::tempdir().unwrap();
    let auth_file = dir.path().join("auth.ini");
    std::fs::write(&auth_file, "user=fileuser\npassword=CNRY-file-6\n").unwrap();
    let in_file = dir.path().join("in.json").to_str().unwrap().to_string();
    std::fs::write(
        &in_file,
        "{\"_index\":\"i\",\"_id\":\"1\",\"_source\":{}}\n",
    )
    .unwrap();
    let hosts = |m: &RouterMock| m.url().replace("http://", "");
    // (label, scheme+host[:port]) error classes
    let targets: Vec<(&str, String)> = vec![
        ("connect", "http://127.0.0.1:1".into()),
        ("dns", "http://no-such-host.invalid:9200".into()),
        ("tls", format!("https://{}", hosts(&m500))),
        ("401", format!("http://{}", hosts(&m401))),
        ("403", format!("http://{}", hosts(&m403))),
        ("500-echo", format!("http://{}", hosts(&m500))),
    ];
    let mut canaries = vec![
        "CNRY-url-1",
        "CNRY-param-2",
        "CNRY-hdr-3",
        "CNRY-pass-4",
        "CNRY-env-5",
        "CNRY-file-6",
        "CNRY-bearer-7",
        "CNRY-in-flag-8: @%#/?é",
        "CNRY-out-flag-9: @%#/?é",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    for (u, p) in [
        ("elastic", "CNRY-url-1"),
        ("envuser", "CNRY-env-5"),
        ("fileuser", "CNRY-file-6"),
        ("flaguser", "CNRY-in-flag-8: @%#/?é"),
        ("flaguser", "CNRY-out-flag-9: @%#/?é"),
    ] {
        canaries.push(base64::engine::general_purpose::STANDARD.encode(format!("{u}:{p}")));
    }
    let mut runs = 0;
    for (class, base) in &targets {
        for channel in [
            "url+params+headers",
            "env",
            "authfile+bearer",
            "input-flags",
            "output-flags",
        ] {
            for debug in [false, true] {
                let out_dir = dir.path().join(format!("o-{class}-{channel}-{debug}"));
                std::fs::create_dir_all(&out_dir).unwrap();
                let mut env: Vec<(&'static str, String)> = vec![];
                let mut args: Vec<String> = vec![];
                let file_out = out_dir.join("out.json").to_str().unwrap().to_string();
                let (input, output) = match channel {
                    "url+params+headers" => {
                        args.extend([
                            "--input-params".into(),
                            r#"{"api_key":"CNRY-param-2"}"#.into(),
                            "--headers".into(),
                            r#"{"X-Api-Key":"CNRY-hdr-3"}"#.into(),
                            "--pass".into(),
                            "CNRY-pass-4".into(),
                        ]);
                        (
                            base.replacen("://", "://elastic:CNRY-url-1@", 1) + "/idx",
                            file_out,
                        )
                    }
                    "env" => {
                        env.push(("SEARCHDUMP_INPUT_USERNAME", "envuser".into()));
                        env.push(("SEARCHDUMP_INPUT_PASSWORD", "CNRY-env-5".into()));
                        (format!("{base}/idx"), file_out)
                    }
                    "input-flags" => {
                        args.extend([
                            "--input-username".into(),
                            "flaguser".into(),
                            "--input-password".into(),
                            "CNRY-in-flag-8: @%#/?é".into(),
                        ]);
                        (format!("{base}/idx"), file_out)
                    }
                    "output-flags" => {
                        args.extend([
                            "--output-username".into(),
                            "flaguser".into(),
                            "--output-password".into(),
                            "CNRY-out-flag-9: @%#/?é".into(),
                        ]);
                        (in_file.clone(), format!("{base}/idx"))
                    }
                    _ => {
                        args.extend([
                            "--httpAuthFile".into(),
                            auth_file.to_str().unwrap().into(),
                            "--input-headers".into(),
                            r#"{"Authorization":"Bearer CNRY-bearer-7"}"#.into(),
                        ]);
                        (format!("{base}/idx"), file_out)
                    }
                };
                args.extend([
                    "--input".into(),
                    input,
                    "--output".into(),
                    output,
                    "--noImplicitRetry".into(),
                    "--requestTimeout".into(),
                    "20".into(),
                    "--idleTimeout".into(),
                    "2".into(),
                ]);
                if debug {
                    args.push("--debug".into());
                }
                let o = run(args, env).await;
                runs += 1;
                let err = stderr(&o);
                let out = String::from_utf8_lossy(&o.stdout).into_owned();
                assert!(!o.status.success(), "{class}/{channel}: should fail: {err}");
                let mut files = String::new();
                for e in std::fs::read_dir(&out_dir).unwrap() {
                    let e = e.unwrap();
                    files.push_str(&e.file_name().to_string_lossy());
                    files.push_str(&std::fs::read_to_string(e.path()).unwrap_or_default());
                }
                for c in &canaries {
                    for (what, text) in [("stderr", &err), ("stdout", &out), ("files", &files)] {
                        assert!(
                            !text.contains(c.as_str()),
                            "{class}/{channel}/debug={debug}: canary {c} in {what}:\n{text}"
                        );
                    }
                }
            }
        }
    }
    assert_eq!(runs, 60);
    // the echo servers really did receive the secrets (the scrubbing is what hid them)
    let seen = m500
        .requests()
        .iter()
        .any(|r| r.query.iter().any(|(_, v)| v == "CNRY-param-2"));
    assert!(seen);
}

/// L-645 / D-4 (TST-BRK-CLI-006): `--input-*` / `--output-*` credentials reach
/// their own endpoint with the documented precedence, hostile characters
/// arrive intact, and no password (or its Basic token) is printed, even with
/// --debug and a server that echoes the request back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn credential_flags_pick_their_endpoint_and_never_leak() {
    use base64::Engine;
    use support::router_mock::{Recorded, Reply, RouterMock};
    let echo = || {
        RouterMock::builder()
            .handler(move |r: Recorded| async move {
                let h: Vec<String> = r.headers.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                Reply::json(
                    401,
                    serde_json::json!({"error": {"type": "echo", "reason": h.join(" | ")}, "status": 401})
                        .to_string(),
                )
            })
            .start()
    };
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("in.json");
    std::fs::write(&data, "{\"_index\":\"i\",\"_id\":\"1\",\"_source\":{}}\n").unwrap();
    let crlf = dir.path().join("crlf.ini");
    std::fs::write(&crlf, "user=file-u\r\npassword=FAKE-file-pw-4\r\n").unwrap();
    let crlf = crlf.to_str().unwrap().to_string();
    let data = data.to_str().unwrap().to_string();
    let out_file = dir.path().join("o.json").to_str().unwrap().to_string();
    let hostile_in = "FAKE-in-pw: @%#/?é 1";
    let hostile_out = "FAKE-out-pw: @%#/?é 3";
    // (label, input is the mock?, extra args, env, expected user:password or None)
    type Case = (
        &'static str,
        bool,
        Vec<String>,
        Vec<(&'static str, String)>,
        Option<String>,
    );
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let cases: Vec<Case> = vec![
        (
            "input flags beat URL",
            true,
            s(&["--input-username", "flag-u", "--input-password", hostile_in]),
            vec![],
            Some(format!("flag-u:{hostile_in}")),
        ),
        (
            "--username/--password alias the input",
            true,
            s(&["--username", "alias-u", "--password", "FAKE-alias-pw-2"]),
            vec![],
            Some("alias-u:FAKE-alias-pw-2".into()),
        ),
        (
            "output flags beat URL",
            false,
            s(&[
                "--output-username",
                "out-u",
                "--output-password",
                hostile_out,
            ]),
            vec![],
            Some(format!("out-u:{hostile_out}")),
        ),
        (
            "output password flag keeps the URL username",
            false,
            s(&["--output-password", hostile_out]),
            vec![],
            Some(format!("url-u:{hostile_out}")),
        ),
        (
            "L-645b: auth file beats URL and env",
            false,
            s(&["--httpAuthFile", &crlf]),
            vec![
                ("SEARCHDUMP_OUTPUT_USERNAME", "env-u".into()),
                ("SEARCHDUMP_OUTPUT_PASSWORD", "FAKE-env-pw-5".into()),
            ],
            Some("file-u:FAKE-file-pw-4".into()),
        ),
        (
            "L-645b: auth file beats the output flags",
            false,
            s(&[
                "--httpAuthFile",
                &crlf,
                "--output-username",
                "out-u",
                "--output-password",
                hostile_out,
            ]),
            vec![],
            Some("file-u:FAKE-file-pw-4".into()),
        ),
        (
            "URL beats env without an auth file",
            false,
            s(&[]),
            vec![
                ("SEARCHDUMP_OUTPUT_USERNAME", "env-u".into()),
                ("SEARCHDUMP_OUTPUT_PASSWORD", "FAKE-env-pw-5".into()),
            ],
            Some("url-u:FAKE-url-pw-9".into()),
        ),
        (
            "CRLF auth file beats env, without the CR",
            false,
            s(&["--httpAuthFile", &crlf, "--noUrlCreds"]),
            vec![
                ("SEARCHDUMP_OUTPUT_USERNAME", "env-u".into()),
                ("SEARCHDUMP_OUTPUT_PASSWORD", "FAKE-env-pw-5".into()),
            ],
            Some("file-u:FAKE-file-pw-4".into()),
        ),
        (
            "env when nothing else",
            false,
            s(&["--noUrlCreds"]),
            vec![
                ("SEARCHDUMP_OUTPUT_USERNAME", "env-u".into()),
                ("SEARCHDUMP_OUTPUT_PASSWORD", "FAKE-env-pw-5".into()),
            ],
            Some("env-u:FAKE-env-pw-5".into()),
        ),
        (
            "--password never reaches the output",
            false,
            s(&["--password", "FAKE-alias-pw-2", "--noUrlCreds"]),
            vec![],
            None,
        ),
        (
            "--input-* never reach the output",
            false,
            s(&[
                "--input-username",
                "in-u",
                "--input-password",
                "FAKE-in-only-6",
                "--noUrlCreds",
            ]),
            vec![],
            None,
        ),
    ];
    let b64 = |v: &str| base64::engine::general_purpose::STANDARD.encode(v);
    for (label, input_is_mock, extra, env, want) in cases {
        for debug in [false, true] {
            let m = echo().await;
            let url_creds = !extra.iter().any(|a| a == "--noUrlCreds");
            let mut target =
                m.url()
                    .replacen("://", "://url-u:FAKE-url-pw-9@", usize::from(url_creds));
            target.push_str("/idx");
            let mut args: Vec<String> = extra
                .iter()
                .filter(|&a| a != "--noUrlCreds")
                .cloned()
                .collect();
            if input_is_mock {
                args.extend([
                    "--input".into(),
                    target,
                    "--output".into(),
                    out_file.clone(),
                ]);
            } else {
                args.extend(["--input".into(), data.clone(), "--output".into(), target]);
            }
            args.extend(s(&["--noImplicitRetry", "--overwrite"]));
            if debug {
                args.push("--debug".into());
            }
            let o = run(args.clone(), env.clone()).await;
            let err = stderr(&o);
            let out = String::from_utf8_lossy(&o.stdout).into_owned();
            assert!(!o.status.success(), "{label}: should fail on 401: {err}");
            let reqs = m.requests();
            assert!(!reqs.is_empty(), "{label}: no request: {err}");
            let got = reqs[0].header("authorization").map(str::to_string);
            assert_eq!(
                got,
                want.as_ref().map(|w| format!("Basic {}", b64(w))),
                "{label}"
            );
            if label.starts_with("L-645b") {
                assert!(
                    err.contains("--httpAuthFile overrides"),
                    "{label}: no override warning: {err}"
                );
            }
            let mut secrets: Vec<String> = vec![
                hostile_in.into(),
                hostile_out.into(),
                "FAKE-alias-pw-2".into(),
                "FAKE-file-pw-4".into(),
                "FAKE-env-pw-5".into(),
                "FAKE-in-only-6".into(),
                "FAKE-url-pw-9".into(),
                "FAKE".into(),
            ];
            if let Some(w) = &want {
                secrets.push(b64(w));
            }
            for c in &secrets {
                for (what, text) in [("stderr", &err), ("stdout", &out)] {
                    assert!(
                        !text.contains(c.as_str()),
                        "{label}/debug={debug}: {c} in {what}:\n{text}"
                    );
                }
            }
        }
    }
}

/// TST-BRK-CLI-006: a world-readable --httpAuthFile is warned about.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn world_readable_auth_file_is_warned_about() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let auth = dir.path().join("auth.ini");
    std::fs::write(&auth, "user=fake\npassword=FAKE-perm-pw\n").unwrap();
    for (mode, warned) in [(0o644, true), (0o600, false)] {
        std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(mode)).unwrap();
        let o = run(
            vec![
                "--input".into(),
                "http://127.0.0.1:1/idx".into(),
                "--output".into(),
                "$".into(),
                "--noImplicitRetry".into(),
                "--httpAuthFile".into(),
                auth.to_str().unwrap().into(),
            ],
            vec![],
        )
        .await;
        let err = stderr(&o);
        assert_eq!(
            err.contains("readable by every user"),
            warned,
            "{mode:o}: {err}"
        );
        assert!(!err.contains("FAKE-perm-pw"), "{err}");
    }
}
