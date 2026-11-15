//! Router mock for Elasticsearch/OpenSearch HTTP (TST-INF-002, L-302).
//!
//! **Routing.** A request is matched on:
//! - method;
//! - path (exact or prefix);
//! - query parameters;
//! - a body predicate.
//!
//! Each route has a response script: a list of [`Reply`] served in order. The last reply repeats unless
//! [`Route::once`] is set. Requests that match no route get the fallback reply (404 JSON).
//!
//! **Connections.** Each connection is served concurrently, with HTTP/1.1 keep-alive. A connection
//! closes after a reply only when the client asked for `Connection: close` or the reply's fault
//! closes it.
//!
//! **Recording.** Every request is recorded in full: method, path, query, headers, decoded body
//! (`Content-Length` or chunked) and the index of the matched route.
//!
//! **Faults.** [`Fault`] covers:
//! - delay before replying;
//! - stall after the headers;
//! - stall mid-body;
//! - truncate at N bytes;
//! - TCP RST;
//! - slow drip;
//! - a wrong `Content-Length`;
//! - bad chunk framing;
//! - `Content-Encoding: gzip` on a plain body;
//! - HTML bodies;
//! - redirects.
//!
//! **Why raw TCP.** The server is written directly on tokio TCP rather than on hyper: hyper refuses to
//! emit malformed responses such as a wrong `Content-Length` or bad chunks, and those are exactly
//! what the fault modes need.
//!
//! [`super::mock_es::MockEs`] (the original sequential script mock) is a thin wrapper over this
//! router.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// What goes wrong while sending a reply.
#[derive(Clone, Debug, Default)]
pub enum Fault {
    #[default]
    None,
    /// Wait before sending anything.
    Delay(Duration),
    /// Send the status line and headers, then stall (then close).
    StallAfterHeaders(Duration),
    /// Send headers and the first `after` body bytes, then stall (then close).
    StallMidBody { after: usize, stall: Duration },
    /// Send only the first `n` bytes of the full response, then close.
    TruncateAt(usize),
    /// Close with a TCP RST (SO_LINGER 0) right after reading the request.
    Reset,
    /// Send the first `n` bytes of the full response, then close with a TCP RST.
    ResetAt(usize),
    /// Send the status line and headers, then FIN (half-close) without the body.
    FinAfterHeaders,
    /// Drop the last `k` bytes of the (possibly gzipped) body; `Content-Length` matches what is
    /// sent, so the HTTP framing is valid but the payload (e.g. the gzip stream) is cut.
    TruncateBodyBy(usize),
    /// Send the response in `chunk`-byte pieces every `every`.
    SlowDrip { chunk: usize, every: Duration },
    /// Advertise `Content-Length` = real length + `delta`, then close.
    WrongContentLength(i64),
    /// `Transfer-Encoding: chunked` with an invalid chunk-size line.
    BadChunk,
    /// Claim `Content-Encoding: gzip` but send the plain body.
    GzipPlainBody,
}

/// One scripted reply.
#[derive(Clone, Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub fault: Fault,
    /// Send real chunked transfer encoding (valid framing).
    pub chunked: bool,
    /// Really gzip the body and send `Content-Encoding: gzip`.
    pub gzip: bool,
}

impl Reply {
    pub fn json(status: u16, body: impl Into<String>) -> Self {
        Reply {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.into().into_bytes(),
            fault: Fault::None,
            chunked: false,
            gzip: false,
        }
    }
    pub fn ok(body: impl Into<String>) -> Self {
        Self::json(200, body)
    }
    /// An HTML page (proxies and load balancers answer like this).
    pub fn html(status: u16, body: impl Into<String>) -> Self {
        let mut r = Self::json(status, body);
        r.headers = vec![("Content-Type".into(), "text/html; charset=utf-8".into())];
        r
    }
    /// A redirect to `location`.
    pub fn redirect(status: u16, location: &str) -> Self {
        let mut r = Self::json(status, "");
        r.headers.push(("Location".into(), location.into()));
        r
    }
    pub fn with_fault(mut self, fault: Fault) -> Self {
        self.fault = fault;
        self
    }
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
    pub fn chunked(mut self) -> Self {
        self.chunked = true;
        self
    }
    pub fn gzipped(mut self) -> Self {
        self.gzip = true;
        self
    }
}

/// A fully recorded request.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Index of the matched route; `None` = fallback.
    pub route: Option<usize>,
    /// Connection number (0-based, in accept order).
    pub conn: usize,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn body_str(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    /// Request line + headers + body, in the format the old `MockEs::requests()` returned.
    pub fn raw_head(&self) -> String {
        let q = if self.query.is_empty() {
            String::new()
        } else {
            format!(
                "?{}",
                self.query
                    .iter()
                    .map(|(k, v)| if v.is_empty() {
                        k.clone()
                    } else {
                        format!("{k}={v}")
                    })
                    .collect::<Vec<_>>()
                    .join("&")
            )
        };
        let mut s = format!("{} {}{} HTTP/1.1\r\n", self.method, self.path, q);
        for (k, v) in &self.headers {
            s.push_str(&format!("{k}: {v}\r\n"));
        }
        s.push_str("\r\n");
        s.push_str(&self.body_str());
        s
    }
}

type BodyPred = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;

/// Dynamic reply for requests no route matched (recording proxy, golden replay).
pub type Handler = Arc<
    dyn Fn(Recorded) -> std::pin::Pin<Box<dyn std::future::Future<Output = Reply> + Send>>
        + Send
        + Sync,
>;

#[derive(Clone)]
enum PathMatch {
    Any,
    Exact(String),
    Prefix(String),
    Contains(String),
}

/// A route: matchers plus a response script.
#[derive(Clone)]
pub struct Route {
    method: Option<String>,
    path: PathMatch,
    query: Vec<(String, Option<String>)>,
    body: Option<BodyPred>,
    replies: Vec<Reply>,
    once: bool,
    served: Arc<AtomicUsize>,
}

impl Route {
    fn new(method: Option<&str>, path: PathMatch) -> Self {
        Route {
            method: method.map(|m| m.to_ascii_uppercase()),
            path,
            query: vec![],
            body: None,
            replies: vec![],
            once: false,
            served: Arc::new(AtomicUsize::new(0)),
        }
    }
    /// Any method, any path.
    pub fn any() -> Self {
        Self::new(None, PathMatch::Any)
    }
    pub fn exact(method: &str, path: &str) -> Self {
        Self::new(Some(method), PathMatch::Exact(path.into()))
    }
    pub fn prefix(method: &str, path: &str) -> Self {
        Self::new(Some(method), PathMatch::Prefix(path.into()))
    }
    /// Path contains `needle` (e.g. `"/_search"`), any method when `method` is `"*"`.
    pub fn contains(method: &str, needle: &str) -> Self {
        let m = (method != "*").then_some(method);
        Self::new(m, PathMatch::Contains(needle.into()))
    }
    /// Require a query parameter (`value` `None` = present with any value).
    pub fn query(mut self, name: &str, value: Option<&str>) -> Self {
        self.query.push((name.into(), value.map(Into::into)));
        self
    }
    pub fn body(mut self, pred: impl Fn(&[u8]) -> bool + Send + Sync + 'static) -> Self {
        self.body = Some(Arc::new(pred));
        self
    }
    pub fn body_contains(self, needle: &str) -> Self {
        let n = needle.as_bytes().to_vec();
        self.body(move |b| b.windows(n.len().max(1)).any(|w| w == n.as_slice()))
    }
    pub fn reply(mut self, r: Reply) -> Self {
        self.replies.push(r);
        self
    }
    pub fn replies(mut self, rs: impl IntoIterator<Item = Reply>) -> Self {
        self.replies.extend(rs);
        self
    }
    /// Serve the script once; after it is exhausted the route stops matching.
    pub fn once(mut self) -> Self {
        self.once = true;
        self
    }

    fn matches(&self, req: &Recorded) -> bool {
        if let Some(m) = &self.method
            && !m.eq_ignore_ascii_case(&req.method)
        {
            return false;
        }
        let ok = match &self.path {
            PathMatch::Any => true,
            PathMatch::Exact(p) => &req.path == p,
            PathMatch::Prefix(p) => req.path.starts_with(p.as_str()),
            PathMatch::Contains(p) => req.path.contains(p.as_str()),
        };
        if !ok {
            return false;
        }
        for (k, v) in &self.query {
            match (req.query_param(k), v) {
                (None, _) => return false,
                (Some(have), Some(want)) if have != want => return false,
                _ => {}
            }
        }
        if let Some(p) = &self.body
            && !p(&req.body)
        {
            return false;
        }
        !(self.once && self.served.load(Ordering::SeqCst) >= self.replies.len())
    }

    fn next_reply(&self) -> Reply {
        let i = self.served.fetch_add(1, Ordering::SeqCst);
        self.replies
            .get(i)
            .or(self.replies.last())
            .cloned()
            .unwrap_or_else(|| Reply::json(500, r#"{"error":"router-mock: route has no replies"}"#))
    }
}

struct Shared {
    routes: Vec<Route>,
    fallback: Reply,
    handler: Option<Handler>,
    recorded: Mutex<Vec<Recorded>>,
    conns: AtomicUsize,
    /// Force `Connection: close` on every reply (the old MockEs behaviour).
    close_always: bool,
    idle_close: Option<(Duration, bool)>,
    idle_closed: AtomicUsize,
    /// 1.2: route `--serverLimits` probe requests like any other (default: answer them 404
    /// here, outside the routes and `requests()`, see [`is_limits_probe`]).
    serve_probes: bool,
    probes: Mutex<Vec<Recorded>>,
}

/// 1.2 (`--serverLimits`): the read-only sizing probe requests (`_nodes/stats`,
/// `_nodes/settings`, `_cat/thread_pool`, `INDEX/_stats/docs,store`). Unless a mock opts in with
/// [`RouterMockBuilder::serve_probes`], they get a 404 (a cluster without node stats: the probe
/// falls back to the fixed 1.1 sizes, which is what every pre-1.2 test expects), are not routed,
/// and are kept out of `requests()` / `request_count()` (see `probe_requests()`).
pub fn is_limits_probe(method: &str, path: &str) -> bool {
    method == "GET"
        && (path.contains("/_nodes/stats")
            || path.contains("/_nodes/settings")
            || path.contains("/_cat/thread_pool")
            || path.ends_with("/_stats/docs,store"))
}

/// A running router mock. Dropping it stops the accept loop.
pub struct RouterMock {
    addr: String,
    shared: Arc<Shared>,
    handle: tokio::task::JoinHandle<()>,
}

/// Builder.
pub struct RouterMockBuilder {
    routes: Vec<Route>,
    fallback: Reply,
    handler: Option<Handler>,
    close_always: bool,
    idle_close: Option<(Duration, bool)>,
    serve_probes: bool,
}

impl RouterMockBuilder {
    /// Route the `--serverLimits` probe requests like any other request (see [`is_limits_probe`]).
    pub fn serve_probes(mut self) -> Self {
        self.serve_probes = true;
        self
    }
    pub fn route(mut self, r: Route) -> Self {
        self.routes.push(r);
        self
    }
    /// Compute the reply for unmatched requests asynchronously (instead of the fallback).
    pub fn handler<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(Recorded) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Reply> + Send + 'static,
    {
        self.handler = Some(Arc::new(move |r| Box::pin(f(r))));
        self
    }
    pub fn fallback(mut self, r: Reply) -> Self {
        self.fallback = r;
        self
    }
    pub fn close_always(mut self) -> Self {
        self.close_always = true;
        self
    }
    /// Close a keep-alive connection that stays idle (no new request) for `after`, like a
    /// load balancer's idle timeout: with a FIN, or with an RST when `rst` (TST-BRK-NET-006).
    pub fn idle_close(mut self, after: Duration, rst: bool) -> Self {
        self.idle_close = Some((after, rst));
        self
    }
    pub async fn start(self) -> RouterMock {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("router mock: bind");
        let addr = listener
            .local_addr()
            .expect("router mock: addr")
            .to_string();
        let shared = Arc::new(Shared {
            routes: self.routes,
            fallback: self.fallback,
            handler: self.handler,
            recorded: Mutex::new(vec![]),
            conns: AtomicUsize::new(0),
            close_always: self.close_always,
            idle_close: self.idle_close,
            idle_closed: AtomicUsize::new(0),
            serve_probes: self.serve_probes,
            probes: Mutex::new(vec![]),
        });
        let handle = tokio::spawn({
            let shared = Arc::clone(&shared);
            async move {
                loop {
                    let Ok((sock, _)) = listener.accept().await else {
                        return;
                    };
                    let conn = shared.conns.fetch_add(1, Ordering::SeqCst);
                    tokio::spawn(serve_conn(sock, conn, Arc::clone(&shared)));
                }
            }
        });
        RouterMock {
            addr,
            shared,
            handle,
        }
    }
}

impl RouterMock {
    pub fn builder() -> RouterMockBuilder {
        RouterMockBuilder {
            routes: vec![],
            fallback: Reply::json(404, r#"{"error":"router-mock: no route"}"#),
            handler: None,
            close_always: false,
            idle_close: None,
            serve_probes: false,
        }
    }
    /// `http://127.0.0.1:PORT`
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
    pub fn addr(&self) -> &str {
        &self.addr
    }
    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.recorded.lock().unwrap().clone()
    }
    pub fn request_count(&self) -> usize {
        self.shared.recorded.lock().unwrap().len()
    }
    /// The `--serverLimits` probe requests answered outside the routes (see [`is_limits_probe`]).
    pub fn probe_requests(&self) -> Vec<Recorded> {
        self.shared.probes.lock().unwrap().clone()
    }
    pub fn connections(&self) -> usize {
        self.shared.conns.load(Ordering::SeqCst)
    }
    /// Connections closed by [`RouterMockBuilder::idle_close`].
    pub fn idle_closed(&self) -> usize {
        self.shared.idle_closed.load(Ordering::SeqCst)
    }
    /// Requests that matched no route.
    pub fn unmatched(&self) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|r| r.route.is_none())
            .collect()
    }
}

impl Drop for RouterMock {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

async fn serve_conn(mut sock: TcpStream, conn: usize, shared: Arc<Shared>) {
    let mut buf = Vec::with_capacity(4096);
    loop {
        let next = match shared.idle_close {
            Some((after, rst)) if buf.is_empty() => {
                // wait for the first byte of the next request at most `after`
                match tokio::time::timeout(after, sock.peek(&mut [0u8; 1])).await {
                    Ok(_) => read_request(&mut sock, &mut buf).await,
                    Err(_) => {
                        shared.idle_closed.fetch_add(1, Ordering::SeqCst);
                        if rst {
                            #[allow(deprecated)]
                            let _ = sock.set_linger(Some(Duration::ZERO));
                        } else {
                            let _ = sock.shutdown().await;
                        }
                        return;
                    }
                }
            }
            _ => read_request(&mut sock, &mut buf).await,
        };
        let Some((mut req, client_close)) = next else {
            return;
        };
        req.conn = conn;
        if !shared.serve_probes && is_limits_probe(&req.method, &req.path) {
            shared.probes.lock().unwrap().push(req);
            let reply = Reply::json(404, r#"{"error":"router-mock: no node stats"}"#);
            let close = client_close || shared.close_always;
            match send_reply(&mut sock, &reply, close).await {
                Ok(true) if !close => continue,
                _ => {
                    let _ = sock.shutdown().await;
                    return;
                }
            }
        }
        let route = shared.routes.iter().position(|r| r.matches(&req));
        req.route = route;
        let reply = match (route, &shared.handler) {
            (Some(i), _) => shared.routes[i].next_reply(),
            (None, Some(h)) => h(req.clone()).await,
            (None, None) => shared.fallback.clone(),
        };
        shared.recorded.lock().unwrap().push(req);
        if let Fault::Reset = reply.fault {
            // SO_LINGER 0 + drop (no FIN first) = RST
            // linger 0 never blocks: the kernel discards the buffer and sends RST
            #[allow(deprecated)]
            let _ = sock.set_linger(Some(Duration::ZERO));
            drop(sock);
            return;
        }
        if let Fault::ResetAt(n) = reply.fault {
            let mut full = reply.clone();
            full.fault = Fault::None;
            let wire = render(&full, true);
            let _ = sock.write_all(&wire[..n.min(wire.len())]).await;
            let _ = sock.flush().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            #[allow(deprecated)]
            let _ = sock.set_linger(Some(Duration::ZERO));
            drop(sock);
            return;
        }
        let close = client_close || shared.close_always;
        match send_reply(&mut sock, &reply, close).await {
            Ok(true) if !close => continue,
            _ => {
                let _ = sock.shutdown().await;
                return;
            }
        }
    }
}

/// Parse one request from the connection (keep-alive aware). Returns the request and
/// whether the client asked to close.
async fn read_request(sock: &mut TcpStream, buf: &mut Vec<u8>) -> Option<(Recorded, bool)> {
    let mut chunk = [0u8; 8192];
    let head_end = loop {
        if let Some(p) = find(buf, b"\r\n\r\n") {
            break p + 4;
        }
        let n = sock.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut rl = lines.next()?.split_whitespace();
    let method = rl.next()?.to_string();
    let target = rl.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            l.split_once(':')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let hget = |n: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(n))
            .map(|(_, v)| v.clone())
    };
    buf.drain(..head_end);
    let body = if hget("transfer-encoding").is_some_and(|v| v.eq_ignore_ascii_case("chunked")) {
        let mut out = vec![];
        loop {
            let line_end = loop {
                if let Some(p) = find(buf, b"\r\n") {
                    break p;
                }
                let n = sock.read(&mut chunk).await.ok()?;
                if n == 0 {
                    return None;
                }
                buf.extend_from_slice(&chunk[..n]);
            };
            let size_txt = String::from_utf8_lossy(&buf[..line_end]).into_owned();
            let size = usize::from_str_radix(size_txt.split(';').next()?.trim(), 16).ok()?;
            buf.drain(..line_end + 2);
            while buf.len() < size + 2 {
                let n = sock.read(&mut chunk).await.ok()?;
                if n == 0 {
                    return None;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            out.extend_from_slice(&buf[..size]);
            buf.drain(..size + 2);
            if size == 0 {
                break;
            }
        }
        out
    } else {
        let len: usize = hget("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        while buf.len() < len {
            let n = sock.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        buf.drain(..len).collect()
    };
    let (path, qs) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = qs
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (k.to_string(), v.to_string())
        })
        .collect();
    let close = hget("connection").is_some_and(|v| v.eq_ignore_ascii_case("close"));
    Some((
        Recorded {
            method,
            path: path.to_string(),
            query,
            headers,
            body,
            route: None,
            conn: 0,
        },
        close,
    ))
}

/// Send a reply. Returns `Ok(true)` when the connection may be reused.
/// The full wire bytes of a reply: (bytes, header length, whether the connection ends).
fn render_parts(r: &Reply, close: bool) -> (Vec<u8>, usize, bool) {
    let mut body = r.body.clone();
    let mut headers: Vec<(String, String)> = r.headers.clone();
    if r.gzip {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let mut e = GzEncoder::new(Vec::new(), Compression::fast());
        e.write_all(&body).expect("gzip");
        body = e.finish().expect("gzip");
        headers.push(("Content-Encoding".into(), "gzip".into()));
    }
    if let Fault::TruncateBodyBy(k) = r.fault {
        body.truncate(body.len().saturating_sub(k));
    }
    if let Fault::GzipPlainBody = r.fault {
        headers.push(("Content-Encoding".into(), "gzip".into()));
    }
    headers.push(("X-Elastic-Product".into(), "Elasticsearch".into()));
    let ends_conn = close
        || !matches!(
            r.fault,
            Fault::None
                | Fault::Delay(_)
                | Fault::GzipPlainBody
                | Fault::SlowDrip { .. }
                | Fault::TruncateBodyBy(_)
        );
    headers.push((
        "Connection".into(),
        if ends_conn { "close" } else { "keep-alive" }.into(),
    ));
    let chunked = r.chunked || matches!(r.fault, Fault::BadChunk);
    if chunked {
        headers.push(("Transfer-Encoding".into(), "chunked".into()));
    } else {
        let len = match r.fault {
            Fault::WrongContentLength(d) => (body.len() as i64 + d).max(0) as usize,
            _ => body.len(),
        };
        headers.push(("Content-Length".into(), len.to_string()));
    }
    let mut head = format!("HTTP/1.1 {} {}\r\n", r.status, reason(r.status));
    for (k, v) in &headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let mut wire = head.into_bytes();
    let head_len = wire.len();
    if chunked {
        if let Fault::BadChunk = r.fault {
            wire.extend_from_slice(b"zz\r\n");
            wire.extend_from_slice(&body);
            wire.extend_from_slice(b"\r\n");
        } else {
            for c in body.chunks(1024) {
                wire.extend_from_slice(format!("{:x}\r\n", c.len()).as_bytes());
                wire.extend_from_slice(c);
                wire.extend_from_slice(b"\r\n");
            }
            wire.extend_from_slice(b"0\r\n\r\n");
        }
    } else {
        wire.extend_from_slice(&body);
    }
    (wire, head_len, ends_conn)
}

fn render(r: &Reply, close: bool) -> Vec<u8> {
    render_parts(r, close).0
}

async fn send_reply(sock: &mut TcpStream, r: &Reply, close: bool) -> std::io::Result<bool> {
    if let Fault::Delay(d) = r.fault {
        tokio::time::sleep(d).await;
    }
    let (wire, head_len, ends_conn) = render_parts(r, close);
    match &r.fault {
        Fault::StallAfterHeaders(d) => {
            sock.write_all(&wire[..head_len]).await?;
            sock.flush().await?;
            tokio::time::sleep(*d).await;
            Ok(false)
        }
        Fault::StallMidBody { after, stall } => {
            let end = (head_len + after).min(wire.len());
            sock.write_all(&wire[..end]).await?;
            sock.flush().await?;
            tokio::time::sleep(*stall).await;
            Ok(false)
        }
        Fault::FinAfterHeaders => {
            sock.write_all(&wire[..head_len]).await?;
            sock.flush().await?;
            Ok(false)
        }
        Fault::TruncateAt(n) => {
            sock.write_all(&wire[..(*n).min(wire.len())]).await?;
            sock.flush().await?;
            Ok(false)
        }
        Fault::SlowDrip { chunk, every } => {
            for c in wire.chunks((*chunk).max(1)) {
                sock.write_all(c).await?;
                sock.flush().await?;
                tokio::time::sleep(*every).await;
            }
            Ok(!ends_conn)
        }
        _ => {
            sock.write_all(&wire).await?;
            sock.flush().await?;
            Ok(!ends_conn)
        }
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        301 => "Moved Permanently",
        302 => "Found",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Status",
    }
}

/// Group recorded requests by `METHOD path` (handy in assertions).
pub fn by_route(reqs: &[Recorded]) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for r in reqs {
        *m.entry(format!("{} {}", r.method, r.path)).or_insert(0) += 1;
    }
    m
}
