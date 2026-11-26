//! edverify: document-set verifier (TST-INF-005). See lib.rs for the digest.
//!
//!   edverify digest [--canonical] [--index-as N] <input>...      # JSON to stdout
//!   edverify compare A.json B.json                                 # exit 1 on mismatch
//!   edverify diff [--canonical] [--index-as N] --a <in>... --b <in>... [--limit 10]
//!
//! An input is a file (NDJSON, .gz, .zst; give every part of a split dump) or
//! a cluster index URL `http(s)://[user:pass@]host:port/<index>`. Credentials
//! can also come from EDV_USER / EDV_PASSWORD.

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use edverify::*;
use sonic_rs::JsonValueTrait;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(
    name = "edverify",
    version,
    about = "Order-independent document-set digest (test-only)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Common {
    /// Hash `_source` re-serialised with sorted keys (cross-engine, pretty input).
    #[arg(long)]
    canonical: bool,
    /// With --canonical: compare numbers by exact decimal value, not spelling
    /// (`1.50` == `1.5` == `15e-1`; a rounded value still differs).
    #[arg(long, requires = "canonical")]
    decimal_numbers: bool,
    /// Use this index name for every document (compare a source and a renamed copy).
    #[arg(long)]
    index_as: Option<String>,
    /// Number of sub-digest buckets.
    #[arg(long, default_value_t = DEFAULT_BUCKETS)]
    buckets: usize,
    /// Hashing threads for file inputs (default: CPUs, max 8).
    #[arg(long)]
    threads: Option<usize>,
    /// Concurrent scroll slices for cluster inputs.
    #[arg(long, default_value_t = 4)]
    slices: usize,
    /// Scroll page size.
    #[arg(long, default_value_t = 1000)]
    size: usize,
    /// Accept any server certificate.
    #[arg(long)]
    insecure: bool,
    /// Extra CA certificate (PEM) for https inputs.
    #[arg(long)]
    ca: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Digest one document set; prints JSON.
    Digest {
        #[command(flatten)]
        c: Common,
        /// Write the JSON here instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(required = true)]
        inputs: Vec<String>,
    },
    /// Compare two digest JSON files.
    Compare { a: PathBuf, b: PathBuf },
    /// Digest A and B; on mismatch re-scan the differing buckets and name ids.
    Diff {
        #[command(flatten)]
        c: Common,
        #[arg(long = "a", required = true, num_args = 1..)]
        a: Vec<String>,
        #[arg(long = "b", required = true, num_args = 1..)]
        b: Vec<String>,
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Also write a JSON report here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn opts(c: &Common) -> Opts {
    Opts {
        canonical: c.canonical,
        decimal_numbers: c.decimal_numbers,
        index_as: c.index_as.clone(),
        buckets: c.buckets,
    }
}

fn threads(c: &Common) -> usize {
    c.threads
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(2, |n| n.get().min(8)))
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn scan(
    inputs: &[String],
    c: &Common,
    sel: Option<HashSet<usize>>,
) -> Result<(SetDigest, Collect)> {
    let o = opts(c);
    if inputs.iter().all(|s| !is_url(s)) {
        let paths: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
        return digest_files(&paths, &o, threads(c), sel);
    }
    if inputs.len() != 1 {
        bail!(
            "give exactly one cluster URL per side (got {})",
            inputs.len()
        );
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads(c).max(2))
        .enable_all()
        .build()?;
    rt.block_on(scan_cluster(&inputs[0], c, &o, sel))
}

struct Target {
    base: reqwest::Url,
    index: String,
    user: Option<String>,
    pass: Option<String>,
}

fn target(s: &str) -> Result<Target> {
    let mut u = reqwest::Url::parse(s).context("bad cluster URL")?;
    let user = (!u.username().is_empty())
        .then(|| u.username().to_string())
        .or_else(|| std::env::var("EDV_USER").ok());
    let pass = u
        .password()
        .map(str::to_string)
        .or_else(|| std::env::var("EDV_PASSWORD").ok());
    let _ = u.set_username("");
    let _ = u.set_password(None);
    let index = u
        .path_segments()
        .and_then(|mut p| p.next_back().map(str::to_string))
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("cluster URL needs an index path"))?;
    let index = percent_decode(&index);
    u.set_path("/");
    Ok(Target {
        base: u,
        index,
        user,
        pass,
    })
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn client(c: &Common) -> Result<reqwest::Client> {
    // reqwest is built without a default crypto provider (L-004): use ring, like searchdump.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut b = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(300));
    if c.insecure {
        b = b.danger_accept_invalid_certs(true);
    }
    if let Some(ca) = &c.ca {
        b = b.add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(ca)?)?);
    }
    Ok(b.build()?)
}

const HTTP_RETRIES: usize = 6;

fn retry_delay(attempt: usize) -> std::time::Duration {
    std::time::Duration::from_millis((250u64 << attempt.min(4)).min(4000))
}

async fn send(
    cl: &reqwest::Client,
    t: &Target,
    method: reqwest::Method,
    path: &str,
    body: String,
) -> Result<Vec<u8>> {
    send_request(cl, t, method, path, body, false).await
}

async fn send_request(
    cl: &reqwest::Client,
    t: &Target,
    method: reqwest::Method,
    path: &str,
    body: String,
    allow_missing: bool,
) -> Result<Vec<u8>> {
    let url = t.base.join(path)?;
    for attempt in 0..=HTTP_RETRIES {
        let mut rq = cl
            .request(method.clone(), url.clone())
            .header("content-type", "application/json")
            .body(body.clone());
        if let Some(u) = &t.user {
            rq = rq.basic_auth(u, t.pass.as_deref());
        }
        // Do not retry transport failures: a continuation may already have
        // advanced the scroll. An explicit 429 is a rejected request, safe to retry.
        let r = rq.send().await?;
        let st = r.status();
        let b = r.bytes().await?.to_vec();
        if st == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < HTTP_RETRIES {
            drop(b);
            tokio::time::sleep(retry_delay(attempt)).await;
            continue;
        }
        if allow_missing && st == reqwest::StatusCode::NOT_FOUND {
            // All supplied scroll IDs already expired/were freed.
            return Ok(Vec::new());
        }
        if !st.is_success() {
            bail!(
                "{path}: HTTP {st}: {}",
                String::from_utf8_lossy(&b[..b.len().min(300)])
            );
        }
        return Ok(b);
    }
    unreachable!("the last failed request returns an error")
}

type ScrollIds = Arc<Mutex<HashSet<String>>>;
type SliceResult = (SetDigest, Collect);

async fn clear_scrolls(cl: &reqwest::Client, t: &Target, seen: &ScrollIds) -> Result<()> {
    let mut ids: Vec<_> = seen
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .cloned()
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    ids.sort();
    let body = serde_json::json!({"scroll_id": ids}).to_string();
    let reply = send_request(cl, t, reqwest::Method::DELETE, "_search/scroll", body, true).await?;
    if !reply.is_empty() {
        let v: serde_json::Value = serde_json::from_slice(&reply)?;
        if v.get("succeeded").and_then(serde_json::Value::as_bool) == Some(false) {
            bail!("server reported scroll cleanup unsuccessful");
        }
    }
    Ok(())
}

// Drain every slice even after an error/panic. Returning on the first error drops
// JoinSet and aborts its siblings, which can own live contexts. Register IDs before
// any fallible page processing, then await cleanup once all tasks have finished.
async fn join_slices(
    mut set: tokio::task::JoinSet<Result<SliceResult>>,
    cl: &reqwest::Client,
    t: &Target,
    seen: ScrollIds,
    sel: Option<HashSet<usize>>,
    buckets: usize,
) -> Result<SliceResult> {
    let mut dig = SetDigest::new(buckets);
    let mut col = Collect {
        buckets: sel,
        ..Default::default()
    };
    let mut error = None;
    while let Some(r) = set.join_next().await {
        match r.map_err(anyhow::Error::from).and_then(|r| r) {
            Ok((d, c2)) => {
                dig.merge(&d);
                for (k, mut v) in c2.docs {
                    col.docs.entry(k).or_default().append(&mut v);
                }
            }
            Err(e) => {
                error.get_or_insert(e);
            }
        }
    }
    let cleanup = clear_scrolls(cl, t, &seen).await;
    if let Some(error) = error {
        return Err(match cleanup {
            Ok(()) => error,
            Err(e) => error.context(format!("scroll cleanup also failed: {e:#}")),
        });
    }
    cleanup.context("scroll cleanup failed")?;
    Ok((dig, col))
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|c| {
            if c.is_ascii_alphanumeric() || b"-_.*,".contains(&c) {
                (c as char).to_string()
            } else {
                format!("%{c:02X}")
            }
        })
        .collect()
}

/// Own sliced-scroll reader (independent of the tool under test).
async fn scan_cluster(
    input: &str,
    c: &Common,
    o: &Opts,
    sel: Option<HashSet<usize>>,
) -> Result<(SetDigest, Collect)> {
    let t = Arc::new(target(input)?);
    let cl = client(c)?;
    let slices = c.slices.max(1);
    let mut set = tokio::task::JoinSet::new();
    let seen: ScrollIds = Arc::new(Mutex::new(HashSet::new()));
    for i in 0..slices {
        let seen = Arc::clone(&seen);
        let (t, cl, o, sel, size) = (t.clone(), cl.clone(), o.clone(), sel.clone(), c.size);
        set.spawn(async move {
            let mut dig = SetDigest::new(o.buckets());
            let mut col = Collect {
                buckets: sel,
                ..Default::default()
            };
            let slice = if slices > 1 {
                format!(r#","slice":{{"id":{i},"max":{slices}}}"#)
            } else {
                String::new()
            };
            let body = format!(r#"{{"size":{size},"sort":["_doc"]{slice}}}"#);
            let path = format!(
                "{}/_search?scroll=5m&expand_wildcards=open,hidden",
                enc(&t.index)
            );
            let mut page = send(&cl, &t, reqwest::Method::POST, &path, body).await?;
            loop {
                let sid = sonic_rs::get(page.as_slice(), &["_scroll_id"])
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string));
                if let Some(s) = &sid {
                    if s.is_empty() || s == "_all" {
                        bail!("server returned an invalid scroll ID");
                    }
                    seen.lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(s.clone());
                }
                let failed = sonic_rs::get(page.as_slice(), &["_shards", "failed"])
                    .ok()
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if failed > 0 {
                    bail!("slice {i}: {failed} shard(s) failed");
                }
                let (d2, c2, n) = tokio::task::spawn_blocking({
                    let o = o.clone();
                    let sel = col.buckets.clone();
                    move || -> Result<(SetDigest, Collect, usize)> {
                        let mut d = SetDigest::new(o.buckets());
                        let mut cc = Collect {
                            buckets: sel,
                            ..Default::default()
                        };
                        let hits = sonic_rs::get(page.as_slice(), &["hits", "hits"])
                            .map_err(|e| anyhow!("no hits.hits: {e}"))?;
                        let mut n = 0;
                        for h in sonic_rs::to_array_iter(hits.as_raw_str().as_bytes()) {
                            let h = h.map_err(|e| anyhow!("bad hit: {e}"))?;
                            digest_one(h.as_raw_str().as_bytes(), &o, &mut d, &mut cc);
                            n += 1;
                        }
                        Ok((d, cc, n))
                    }
                })
                .await??;
                dig.merge(&d2);
                for (k, mut v) in c2.docs {
                    col.docs.entry(k).or_default().append(&mut v);
                }
                let Some(sid) = sid else { break };
                if n == 0 {
                    break;
                }
                let body = format!(
                    r#"{{"scroll":"5m","scroll_id":{}}}"#,
                    sonic_rs::to_string(&sid)?
                );
                page = send(&cl, &t, reqwest::Method::POST, "_search/scroll", body).await?;
            }
            Ok::<_, anyhow::Error>((dig, col))
        });
    }
    let (dig, col) = join_slices(set, &cl, &t, seen, sel, o.buckets()).await?;
    // Cross-check with _count (catches a reader that silently stopped early).
    let cnt = send(
        &cl,
        &t,
        reqwest::Method::GET,
        &format!("{}/_count?expand_wildcards=open,hidden", enc(&t.index)),
        String::new(),
    )
    .await?;
    let n = sonic_rs::get(cnt.as_slice(), &["count"])
        .ok()
        .and_then(|v| v.as_u64())
        .unwrap_or(u64::MAX);
    if n != dig.count {
        bail!("scroll returned {} docs but _count says {n}", dig.count);
    }
    Ok((dig, col))
}

fn redact(s: &str) -> String {
    match reqwest::Url::parse(s) {
        Ok(mut u) if u.password().is_some() => {
            let _ = u.set_password(Some("***"));
            u.to_string()
        }
        _ => s.to_string(),
    }
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("edverify: error: {e:#}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Digest { c, out, inputs } => {
            let t0 = std::time::Instant::now();
            let (d, _) = scan(&inputs, &c, None)?;
            let secs = t0.elapsed().as_secs_f64();
            let specs: Vec<String> = inputs.iter().map(|s| redact(s)).collect();
            let mut j = d.to_json(&specs, &opts(&c));
            j["seconds"] = serde_json::json!(secs);
            let s = serde_json::to_string_pretty(&j)?;
            match out {
                Some(p) => std::fs::write(p, s + "\n")?,
                None => println!("{s}"),
            }
            eprintln!(
                "edverify: count={} digest={} bad_lines={} ({:.0} docs/s)",
                d.count,
                d.short(),
                d.bad_lines,
                d.count as f64 / secs.max(1e-9)
            );
            Ok(if d.bad_lines > 0 { 1 } else { 0 })
        }
        Cmd::Compare { a, b } => {
            let ja: serde_json::Value = serde_json::from_slice(&std::fs::read(&a)?)?;
            let jb: serde_json::Value = serde_json::from_slice(&std::fs::read(&b)?)?;
            let mut same = true;
            for k in ["mode", "count", "sum", "xor", "buckets"] {
                if ja[k] != jb[k] {
                    println!("{k}: {} != {}", ja[k], jb[k]);
                    same = false;
                }
            }
            if let (Some(x), Some(y)) = (ja["bucket_fp"].as_array(), jb["bucket_fp"].as_array()) {
                let diff: Vec<usize> = (0..x.len().min(y.len()))
                    .filter(|&i| x[i] != y[i])
                    .collect();
                if !diff.is_empty() {
                    println!(
                        "mismatched buckets ({}): {:?}",
                        diff.len(),
                        &diff[..diff.len().min(20)]
                    );
                }
            }
            println!("{}", if same { "EQUAL" } else { "DIFFERENT" });
            Ok(if same { 0 } else { 1 })
        }
        Cmd::Diff {
            c,
            a,
            b,
            limit,
            out,
        } => {
            let (da, _) = scan(&a, &c, None)?;
            let (db, _) = scan(&b, &c, None)?;
            let mm = mismatched_buckets(&da, &db)?;
            let equal =
                mm.is_empty() && da.count == db.count && da.sum == db.sum && da.xor == db.xor;
            println!(
                "A: count={} digest={}  B: count={} digest={}",
                da.count,
                da.short(),
                db.count,
                db.short()
            );
            let mut diffs = Vec::new();
            if !equal {
                println!("mismatched buckets: {}", mm.len());
                let sel: HashSet<usize> = mm.iter().copied().collect();
                // Re-scan only the mismatched buckets; stop early once `limit`
                // ids are named (buckets are scanned in batches of 64).
                let mut order: Vec<usize> = mm.clone();
                order.sort();
                for batch in order.chunks(64) {
                    let s: HashSet<usize> =
                        batch.iter().copied().filter(|x| sel.contains(x)).collect();
                    let (_, ca) = scan(&a, &c, Some(s.clone()))?;
                    let (_, cb) = scan(&b, &c, Some(s))?;
                    diffs.extend(id_diffs(&ca, &cb, limit - diffs.len()));
                    if diffs.len() >= limit {
                        break;
                    }
                }
                for d in &diffs {
                    println!("  {d}");
                }
            }
            println!("{}", if equal { "EQUAL" } else { "DIFFERENT" });
            if let Some(p) = out {
                let j = serde_json::json!({
                    "equal": equal, "a": {"count": da.count, "digest": da.short(), "bad_lines": da.bad_lines},
                    "b": {"count": db.count, "digest": db.short(), "bad_lines": db.bad_lines},
                    "mismatched_buckets": mm.len(),
                    "first_differences": diffs.iter().map(|d| d.to_string()).collect::<Vec<_>>(),
                });
                std::fs::write(p, serde_json::to_string_pretty(&j)? + "\n")?;
            }
            Ok(if equal && da.bad_lines == 0 && db.bad_lines == 0 {
                0
            } else {
                1
            })
        }
    }
}

#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../tests/support/router_mock.rs"]
mod router_mock;

#[cfg(test)]
mod cluster_tests;
