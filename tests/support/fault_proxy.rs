//! In-process TCP fault proxy with a policy DSL (TST-INF-004, L-304).
//!
//! Promoted from `tests/it/concurrency.rs`. A [`Policy`] says what goes wrong on each
//! proxied connection, per direction (`Up` = client -> server, `Down` = server -> client):
//!
//! | DSL key | Builder | Effect | toxiproxy toxic |
//! |---|---|---|---|
//! | `refuse_every=N` | [`Policy::refuse_every`] | every N-th connection is closed at once | — (in-process only) |
//! | `cut_up=B` / `cut_down=B` | [`Policy::cut_after`] | forward B bytes in that direction, then close | `limit_data` |
//! | `latency_up=MS` / `latency_down=MS` | [`Policy::latency`] | delay each chunk by MS | `latency` |
//! | `bw_up=KBPS` / `bw_down=KBPS` | [`Policy::bandwidth`] | cap throughput at KBPS KiB/s | `bandwidth` |
//! | `half_close_up=B` / `half_close_down=B` | [`Policy::half_close`] | after B bytes, shut that direction's write half (FIN) but keep the other direction open | — (in-process only) |
//! | `reset` | [`Policy::reset`] | cuts send a TCP RST instead of FIN | `reset_peer` (approx.) |
//!
//! `Policy::parse("refuse_every=5;cut_down=4096;latency_down=20")` reads the DSL (`;` or `,`
//! separated). [`Policy::cut_with`] takes a function of the connection number for varying cuts
//! (the original concurrency test's pattern). The same policies drive the toxiproxy container
//! (`scripts/it/toxiproxy.sh`) through [`toxiproxy::Toxiproxy`], and `tests/fault_conformance.rs`
//! runs one conformance suite against both.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// client -> server (the request)
    Up,
    /// server -> client (the response)
    Down,
}

type CutFn = Arc<dyn Fn(u64) -> Option<u64> + Send + Sync>;

#[derive(Clone, Default)]
struct DirPolicy {
    cut: Option<CutFn>,
    latency: Option<Duration>,
    bw_kbps: Option<u64>,
    half_close: Option<u64>,
}

/// What goes wrong on proxied connections.
#[derive(Clone, Default)]
pub struct Policy {
    refuse_every: Option<u64>,
    up: DirPolicy,
    down: DirPolicy,
    reset: bool,
}

impl std::fmt::Debug for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Policy(refuse_every={:?}, reset={})",
            self.refuse_every, self.reset
        )
    }
}

impl Policy {
    pub fn new() -> Self {
        Self::default()
    }
    fn dir(&mut self, d: Dir) -> &mut DirPolicy {
        match d {
            Dir::Up => &mut self.up,
            Dir::Down => &mut self.down,
        }
    }
    pub fn refuse_every(mut self, n: u64) -> Self {
        self.refuse_every = Some(n.max(1));
        self
    }
    pub fn cut_after(mut self, d: Dir, bytes: u64) -> Self {
        self.dir(d).cut = Some(Arc::new(move |_| Some(bytes)));
        self
    }
    /// Cut after `f(connection_number)` bytes (1-based connection numbers; `None` = no cut).
    pub fn cut_with(
        mut self,
        d: Dir,
        f: impl Fn(u64) -> Option<u64> + Send + Sync + 'static,
    ) -> Self {
        self.dir(d).cut = Some(Arc::new(f));
        self
    }
    pub fn latency(mut self, d: Dir, l: Duration) -> Self {
        self.dir(d).latency = Some(l);
        self
    }
    pub fn bandwidth(mut self, d: Dir, kib_per_s: u64) -> Self {
        self.dir(d).bw_kbps = Some(kib_per_s.max(1));
        self
    }
    pub fn half_close(mut self, d: Dir, after_bytes: u64) -> Self {
        self.dir(d).half_close = Some(after_bytes);
        self
    }
    pub fn reset(mut self) -> Self {
        self.reset = true;
        self
    }

    /// Parse the DSL, e.g. `"refuse_every=5;cut_down=4096;latency_down=20;bw_up=64;reset"`.
    pub fn parse(s: &str) -> Result<Policy, String> {
        let mut p = Policy::new();
        for item in s.split([';', ',']).map(str::trim).filter(|x| !x.is_empty()) {
            let (k, v) = item.split_once('=').unwrap_or((item, ""));
            let num = || {
                v.parse::<u64>()
                    .map_err(|_| format!("{k}: `{v}` is not a number"))
            };
            let dir = |suffix: &str| match suffix {
                "up" => Ok(Dir::Up),
                "down" => Ok(Dir::Down),
                _ => Err(format!("unknown direction in `{k}`")),
            };
            p = match k.rsplit_once('_') {
                _ if k == "refuse_every" => p.refuse_every(num()?),
                _ if k == "reset" => p.reset(),
                Some(("cut", d)) => p.cut_after(dir(d)?, num()?),
                Some(("latency", d)) => p.latency(dir(d)?, Duration::from_millis(num()?)),
                Some(("bw", d)) => p.bandwidth(dir(d)?, num()?),
                Some(("half_close", d)) => p.half_close(dir(d)?, num()?),
                _ => return Err(format!("unknown fault policy key `{k}`")),
            };
        }
        Ok(p)
    }
}

/// A running proxy. Dropping it stops accepting.
pub struct FaultProxy {
    pub port: u16,
    faults: Arc<AtomicU64>,
    conns: Arc<AtomicU64>,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for FaultProxy {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

impl FaultProxy {
    /// Proxy `127.0.0.1:<port>` to `upstream` (`host:port`).
    pub async fn start(upstream: &str, policy: Policy) -> FaultProxy {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let faults = Arc::new(AtomicU64::new(0));
        let conns = Arc::new(AtomicU64::new(0));
        let (f2, c2, upstream) = (
            Arc::clone(&faults),
            Arc::clone(&conns),
            upstream.to_string(),
        );
        let handle = tokio::spawn(async move {
            loop {
                let Ok((down, _)) = l.accept().await else {
                    return;
                };
                let n = c2.fetch_add(1, Ordering::SeqCst) + 1;
                if policy.refuse_every.is_some_and(|k| n % k == 0) {
                    f2.fetch_add(1, Ordering::SeqCst);
                    close(down, policy.reset);
                    continue;
                }
                tokio::spawn(serve(
                    down,
                    upstream.clone(),
                    policy.clone(),
                    n,
                    Arc::clone(&f2),
                ));
            }
        });
        FaultProxy {
            port,
            faults,
            conns,
            handle,
        }
    }
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
    /// Faults injected so far (refusals + cuts + half-closes).
    pub fn faults(&self) -> u64 {
        self.faults.load(Ordering::SeqCst)
    }
    pub fn connections(&self) -> u64 {
        self.conns.load(Ordering::SeqCst)
    }
}

fn close(s: TcpStream, reset: bool) {
    if reset {
        // linger 0 never blocks: the kernel discards the buffer and sends RST
        #[allow(deprecated)]
        let _ = s.set_linger(Some(Duration::ZERO));
    }
    drop(s);
}

enum End {
    Eof,
    Cut,
    HalfClosed,
}

/// Forward one direction. Returns how it ended and the write half, so a cut with `reset` can
/// close the socket without a FIN (`forget` + dropping the read half => RST under linger 0).
async fn pump(
    mut r: OwnedReadHalf,
    mut w: OwnedWriteHalf,
    p: DirPolicy,
    n: u64,
) -> std::io::Result<(End, OwnedWriteHalf)> {
    let cut = p.cut.as_ref().and_then(|f| f(n));
    let mut sent: u64 = 0;
    let mut buf = vec![0u8; 4096];
    loop {
        let k = r.read(&mut buf).await?;
        if k == 0 {
            let _ = w.shutdown().await;
            return Ok((End::Eof, w));
        }
        let mut chunk = &buf[..k];
        if let Some(l) = p.latency {
            tokio::time::sleep(l).await;
        }
        let limit = match (cut, p.half_close) {
            (Some(c), Some(h)) => Some((c.min(h), c <= h)),
            (Some(c), None) => Some((c, true)),
            (None, Some(h)) => Some((h, false)),
            (None, None) => None,
        };
        let mut stop = None;
        if let Some((lim, is_cut)) = limit
            && sent + chunk.len() as u64 >= lim
        {
            chunk = &chunk[..(lim - sent) as usize];
            stop = Some(is_cut);
        }
        if let Some(kbps) = p.bw_kbps {
            tokio::time::sleep(Duration::from_secs_f64(
                chunk.len() as f64 / (kbps as f64 * 1024.0),
            ))
            .await;
        }
        w.write_all(chunk).await?;
        sent += chunk.len() as u64;
        match stop {
            Some(true) => return Ok((End::Cut, w)),
            Some(false) => {
                let _ = w.shutdown().await;
                return Ok((End::HalfClosed, w));
            }
            None => {}
        }
    }
}

async fn serve(down: TcpStream, upstream: String, p: Policy, n: u64, faults: Arc<AtomicU64>) {
    let Ok(up) = TcpStream::connect(&upstream).await else {
        return;
    };
    if p.reset {
        // every close of this connection becomes an RST (linger 0 never blocks)
        #[allow(deprecated)]
        let _ = (
            down.set_linger(Some(Duration::ZERO)),
            up.set_linger(Some(Duration::ZERO)),
        );
    }
    let (dr, dw) = down.into_split();
    let (ur, uw) = up.into_split();
    let mut c2u = tokio::spawn(pump(dr, uw, p.up.clone(), n));
    let mut u2c = tokio::spawn(pump(ur, dw, p.down.clone(), n));
    // A cut tears the whole connection down; EOF and half-close let the other direction finish.
    let (first, up_first) = tokio::select! {
        r = &mut c2u => (r, true),
        r = &mut u2c => (r, false),
    };
    let other = if up_first { &mut u2c } else { &mut c2u };
    match first {
        Ok(Ok((End::Cut, w))) => {
            faults.fetch_add(1, Ordering::SeqCst);
            if p.reset {
                w.forget(); // no FIN: the socket closes when its read half drops (RST)
            } else {
                drop(w);
            }
            other.abort();
            let _ = other.await;
        }
        Ok(Ok((end, w))) => {
            drop(w);
            if matches!(end, End::HalfClosed) {
                faults.fetch_add(1, Ordering::SeqCst);
            }
            if let Ok(Ok((End::Cut | End::HalfClosed, _))) = other.await {
                faults.fetch_add(1, Ordering::SeqCst);
            }
        }
        _ => {
            other.abort();
            let _ = other.await;
        }
    }
}

/// Client for the toxiproxy container (`scripts/it/toxiproxy.sh`, API at `TOXIPROXY_URL`).
pub mod toxiproxy {
    use super::{Dir, Policy};
    use serde_json::json;

    pub struct Toxiproxy {
        api: String,
        client: reqwest::Client,
    }

    pub struct Proxy {
        api: String,
        name: String,
        client: reqwest::Client,
        pub listen: String,
    }

    impl Toxiproxy {
        pub fn from_env() -> Option<Toxiproxy> {
            let api = std::env::var("TOXIPROXY_URL")
                .ok()
                .filter(|s| !s.is_empty())?;
            Some(Toxiproxy {
                api: api.trim_end_matches('/').into(),
                client: {
                    searchdump_rs::http::tls::install_default_provider();
                    reqwest::Client::new()
                },
            })
        }

        /// Create a proxy `listen` -> `upstream` with the toxics equivalent to `policy`.
        /// Errors for policy items toxiproxy cannot express (refuse_every, half_close, varying cuts).
        pub async fn proxy(
            &self,
            name: &str,
            listen: &str,
            upstream: &str,
            policy: &Policy,
        ) -> Result<Proxy, String> {
            if policy.refuse_every.is_some() {
                return Err("refuse_every has no toxiproxy equivalent".into());
            }
            let _ = self
                .client
                .delete(format!("{}/proxies/{name}", self.api))
                .send()
                .await;
            let r = self
                .client
                .post(format!("{}/proxies", self.api))
                .json(
                    &json!({"name": name, "listen": listen, "upstream": upstream, "enabled": true}),
                )
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !r.status().is_success() {
                return Err(format!(
                    "create proxy: {} {}",
                    r.status(),
                    r.text().await.unwrap_or_default()
                ));
            }
            let p = Proxy {
                api: self.api.clone(),
                name: name.into(),
                client: self.client.clone(),
                listen: listen.into(),
            };
            for (d, dp) in [(Dir::Up, &policy.up), (Dir::Down, &policy.down)] {
                let stream = if d == Dir::Up {
                    "upstream"
                } else {
                    "downstream"
                };
                if dp.half_close.is_some() {
                    return Err("half_close has no toxiproxy equivalent".into());
                }
                if let Some(l) = dp.latency {
                    p.toxic(
                        stream,
                        "latency",
                        json!({"latency": l.as_millis() as u64, "jitter": 0}),
                    )
                    .await?;
                }
                if let Some(bw) = dp.bw_kbps {
                    p.toxic(stream, "bandwidth", json!({"rate": bw})).await?;
                }
                if let Some(f) = &dp.cut {
                    let (a, b) = (f(1), f(2));
                    if a != b {
                        return Err("varying cuts have no toxiproxy equivalent".into());
                    }
                    if let Some(bytes) = a {
                        p.toxic(stream, "limit_data", json!({"bytes": bytes}))
                            .await?;
                    }
                }
            }
            if policy.reset {
                p.toxic("downstream", "reset_peer", json!({"timeout": 0}))
                    .await?;
            }
            Ok(p)
        }
    }

    impl Proxy {
        async fn toxic(
            &self,
            stream: &str,
            kind: &str,
            attributes: serde_json::Value,
        ) -> Result<(), String> {
            let r = self
                .client
                .post(format!("{}/proxies/{}/toxics", self.api, self.name))
                .json(&json!({"name": format!("{kind}_{stream}"), "type": kind, "stream": stream, "toxicity": 1.0, "attributes": attributes}))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if r.status().is_success() {
                Ok(())
            } else {
                Err(format!(
                    "toxic {kind}: {} {}",
                    r.status(),
                    r.text().await.unwrap_or_default()
                ))
            }
        }

        pub async fn delete(self) {
            let _ = self
                .client
                .delete(format!("{}/proxies/{}", self.api, self.name))
                .send()
                .await;
        }
    }
}
