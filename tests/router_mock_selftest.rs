//! Self-tests for the router mock (TST-INF-002, L-302): routing, recording, keep-alive,
//! concurrency, and every fault mode.
mod support;

use std::time::{Duration, Instant};

use support::router_mock::{Fault, Reply, Route, RouterMock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn raw(mock: &RouterMock, req: &str) -> std::io::Result<Vec<u8>> {
    let mut s = TcpStream::connect(mock.addr()).await?;
    s.write_all(req.as_bytes()).await?;
    let mut out = vec![];
    s.read_to_end(&mut out).await?;
    Ok(out)
}

const GET: &str = "GET /x HTTP/1.1\r\nHost: m\r\nConnection: close\r\n\r\n";

async fn one(fault: Fault, body: &str) -> RouterMock {
    RouterMock::builder()
        .route(Route::any().reply(Reply::ok(body).with_fault(fault)))
        .start()
        .await
}

#[tokio::test]
async fn routes_match_method_path_query_and_body_and_record_everything() {
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/").reply(Reply::ok(r#"{"v":1}"#)))
        .route(
            Route::contains("POST", "/_search")
                .query("scroll", Some("1m"))
                .reply(Reply::ok("scroll")),
        )
        .route(
            Route::contains("POST", "/_search")
                .body_contains("\"pit\"")
                .reply(Reply::ok("pit")),
        )
        .route(
            Route::prefix("DELETE", "/_search/scroll")
                .reply(Reply::ok("a"))
                .reply(Reply::ok("b"))
                .once(),
        )
        .start()
        .await;
    let c = {
        searchdump_rs::http::tls::install_default_provider();
        reqwest::Client::new()
    };
    let u = m.url();
    assert_eq!(
        c.get(format!("{u}/"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        r#"{"v":1}"#
    );
    assert_eq!(
        c.post(format!("{u}/i/_search?scroll=1m"))
            .body("{}")
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "scroll"
    );
    assert_eq!(
        c.post(format!("{u}/_search"))
            .body(r#"{"pit":{}}"#)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "pit"
    );
    assert_eq!(
        c.delete(format!("{u}/_search/scroll"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "a"
    );
    assert_eq!(
        c.delete(format!("{u}/_search/scroll"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "b"
    );
    // once(): exhausted -> fallback 404
    assert_eq!(
        c.delete(format!("{u}/_search/scroll"))
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    let reqs = m.requests();
    assert_eq!(reqs.len(), 6);
    assert_eq!(reqs[1].query_param("scroll"), Some("1m"));
    assert_eq!(reqs[2].body_str(), r#"{"pit":{}}"#);
    assert_eq!(reqs[5].route, None);
    assert_eq!(m.unmatched().len(), 1);
    assert!(reqs[0].header("host").is_some());
}

#[tokio::test]
async fn chunked_request_bodies_are_decoded() {
    let m = RouterMock::builder()
        .route(Route::any().reply(Reply::ok("{}")))
        .start()
        .await;
    let out = raw(&m, "POST /_bulk HTTP/1.1\r\nHost: m\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n")
        .await
        .unwrap();
    assert!(String::from_utf8_lossy(&out).starts_with("HTTP/1.1 200"));
    assert_eq!(m.requests()[0].body_str(), "hello world");
}

#[tokio::test]
async fn keep_alive_reuses_one_connection() {
    let m = RouterMock::builder()
        .route(Route::any().reply(Reply::ok("{}")))
        .start()
        .await;
    let c = {
        searchdump_rs::http::tls::install_default_provider();
        reqwest::Client::new()
    };
    for _ in 0..5 {
        c.get(format!("{}/", m.url()))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
    }
    assert_eq!(m.request_count(), 5);
    assert_eq!(m.connections(), 1);
}

#[tokio::test]
async fn concurrent_connections_are_served_in_parallel() {
    let m = one(Fault::Delay(Duration::from_millis(300)), "{}").await;
    let c = {
        searchdump_rs::http::tls::install_default_provider();
        reqwest::Client::new()
    };
    let t = Instant::now();
    let tasks: Vec<_> = (0..8)
        .map(|_| tokio::spawn(c.get(format!("{}/", m.url())).send()))
        .collect();
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(m.request_count(), 8);
}

#[tokio::test]
async fn delay() {
    let m = one(Fault::Delay(Duration::from_millis(250)), "{}").await;
    let t = Instant::now();
    raw(&m, GET).await.unwrap();
    assert!(t.elapsed() >= Duration::from_millis(250));
}

#[tokio::test]
async fn stall_after_headers() {
    let m = one(
        Fault::StallAfterHeaders(Duration::from_secs(5)),
        r#"{"a":1}"#,
    )
    .await;
    let mut s = TcpStream::connect(m.addr()).await.unwrap();
    s.write_all(GET.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; 4096];
    let n = s.read(&mut buf).await.unwrap();
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    assert!(head.ends_with("\r\n\r\n"), "{head}");
    assert!(
        tokio::time::timeout(Duration::from_millis(300), s.read(&mut buf))
            .await
            .is_err(),
        "body must stall"
    );
}

#[tokio::test]
async fn stall_mid_body() {
    let m = one(
        Fault::StallMidBody {
            after: 3,
            stall: Duration::from_secs(5),
        },
        r#"{"abc":1}"#,
    )
    .await;
    let mut s = TcpStream::connect(m.addr()).await.unwrap();
    s.write_all(GET.as_bytes()).await.unwrap();
    let mut got = vec![];
    let mut buf = [0u8; 4096];
    while !String::from_utf8_lossy(&got).contains("\r\n\r\n{\"a") {
        let n = s.read(&mut buf).await.unwrap();
        got.extend_from_slice(&buf[..n]);
    }
    assert!(String::from_utf8_lossy(&got).ends_with("\r\n\r\n{\"a"));
    assert!(
        tokio::time::timeout(Duration::from_millis(300), s.read(&mut buf))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn truncate_at() {
    let m = one(Fault::TruncateAt(20), r#"{"long":"body"}"#).await;
    assert_eq!(raw(&m, GET).await.unwrap().len(), 20);
}

#[tokio::test]
async fn reset() {
    let m = one(Fault::Reset, "{}").await;
    let e = raw(&m, GET).await.expect_err("expected RST");
    assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset, "{e}");
    assert_eq!(m.request_count(), 1);
}

#[tokio::test]
async fn slow_drip() {
    let m = one(
        Fault::SlowDrip {
            chunk: 40,
            every: Duration::from_millis(50),
        },
        r#"{"drip":true}"#,
    )
    .await;
    let t = Instant::now();
    let out = raw(&m, GET).await.unwrap();
    let n = out.len().div_ceil(40) as u32;
    assert!(t.elapsed() >= Duration::from_millis(50) * (n - 1));
    assert!(String::from_utf8_lossy(&out).ends_with(r#"{"drip":true}"#));
}

#[tokio::test]
async fn wrong_content_length() {
    let m = one(Fault::WrongContentLength(10), r#"{"a":1}"#).await;
    let out = String::from_utf8_lossy(&raw(&m, GET).await.unwrap()).into_owned();
    assert!(out.contains("Content-Length: 17\r\n"), "{out}");
    let r = tls_get(format!("{}/", m.url())).await.unwrap();
    assert!(r.text().await.is_err(), "short body must be an error");
}

#[tokio::test]
async fn bad_chunk_framing() {
    let m = one(Fault::BadChunk, r#"{"a":1}"#).await;
    let out = String::from_utf8_lossy(&raw(&m, GET).await.unwrap()).into_owned();
    assert!(out.contains("Transfer-Encoding: chunked") && out.contains("\r\n\r\nzz\r\n"));
    let r = tls_get(format!("{}/", m.url())).await;
    assert!(r.is_err() || r.unwrap().text().await.is_err());
}

#[tokio::test]
async fn gzip_header_with_plain_body() {
    let m = one(Fault::GzipPlainBody, r#"{"a":1}"#).await;
    let out = String::from_utf8_lossy(&raw(&m, GET).await.unwrap()).into_owned();
    assert!(out.contains("Content-Encoding: gzip") && out.ends_with(r#"{"a":1}"#));
    let r = {
        searchdump_rs::http::tls::install_default_provider();
        reqwest::Client::builder()
    }
    .gzip(true)
    .build()
    .unwrap()
    .get(format!("{}/", m.url()))
    .send()
    .await
    .unwrap();
    assert!(
        r.text().await.is_err(),
        "plain body under gzip header must fail to decode"
    );
}

#[tokio::test]
async fn real_gzip_and_valid_chunking_round_trip() {
    let big = format!("{{\"x\":\"{}\"}}", "y".repeat(5000));
    let m = RouterMock::builder()
        .route(Route::any().reply(Reply::ok(big.clone()).gzipped().chunked()))
        .start()
        .await;
    let r = {
        searchdump_rs::http::tls::install_default_provider();
        reqwest::Client::builder()
    }
    .gzip(true)
    .build()
    .unwrap()
    .get(format!("{}/", m.url()))
    .send()
    .await
    .unwrap();
    assert_eq!(r.text().await.unwrap(), big);
}

#[tokio::test]
async fn html_bodies() {
    let m = RouterMock::builder()
        .route(Route::any().reply(Reply::html(502, "<html><body>Bad Gateway</body></html>")))
        .start()
        .await;
    let r = tls_get(format!("{}/", m.url())).await.unwrap();
    assert_eq!(r.status(), 502);
    assert!(
        r.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
}

#[tokio::test]
async fn redirects() {
    let m = RouterMock::builder()
        .route(Route::exact("GET", "/old").reply(Reply::redirect(302, "/new")))
        .route(Route::exact("GET", "/new").reply(Reply::ok("moved")))
        .start()
        .await;
    let r = tls_get(format!("{}/old", m.url())).await.unwrap();
    assert_eq!(r.text().await.unwrap(), "moved");
    let paths: Vec<_> = m.requests().iter().map(|r| r.path.clone()).collect();
    assert_eq!(paths, ["/old", "/new"]);
}

// ---- chaos seed helper (INF-013) ----

#[test]
fn chaos_seed_parses_and_replays() {
    use support::chaos::{Rng, parse};
    assert_eq!(parse("42"), Some(42));
    assert_eq!(parse("0x2A"), Some(42));
    assert_eq!(parse("nope"), None);
    let a: Vec<u64> = {
        let mut r = Rng::new(7);
        (0..5).map(|_| r.next_u64()).collect()
    };
    let b: Vec<u64> = {
        let mut r = Rng::new(7);
        (0..5).map(|_| r.next_u64()).collect()
    };
    assert_eq!(a, b, "same seed, same sequence");
    assert_ne!(a, {
        let mut r = Rng::new(8);
        (0..5).map(|_| r.next_u64()).collect::<Vec<_>>()
    });
    let mut r = Rng::new(1);
    assert!((0..1000).all(|_| r.below(10) < 10));
}

/// L4-004: reqwest is built without a default crypto provider; install ring first.
async fn tls_get(url: String) -> reqwest::Result<reqwest::Response> {
    searchdump_rs::http::tls::install_default_provider();
    reqwest::get(url).await
}
