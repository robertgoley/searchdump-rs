//! searchdump-multi: dump or load many indices at once with searchdump.
//!
//! Each selected index gets its own settings, mapping, optional alias and data files, in the
//! layout that `scripts/oslogs.py` writes (`INDEX.settings.json`, `INDEX.mapping.json`,
//! `INDEX.data.json.gz` / `.zst`), so a directory written by either tool loads with either.
//! Indices are selected by `--match` (and `--exclude`) regexes and can be broken out into
//! per-index or per-group subdirectories (`--layout`). The per-index work runs as child
//! `searchdump` processes, at most `--parallel` indices at a time; one index failing does not
//! stop the others. A manifest (`multielasticdump.json`) records what was dumped.
//!
//! `--matchType` also selects indices by alias or whole data streams; cluster-wide templates,
//! policies and stored scripts go to `DIR/_cluster/` and load before any index; `--fileSize` /
//! `--maxRows` split data files into parts that load in order.
//!
//! `--nodeCompat` writes the file names Node's `multielasticdump` 6.x reads (`INDEX.json.gz`,
//! `INDEX.settings.json.gz`, ...) with the manifest and `_cluster/` moved to `DIR.meta` (`--metaDir`),
//! because Node treats every entry in the directory as an index. Load accepts both namings.
//!
//! Credentials: use `SEARCHDUMP_INPUT_*` / `SEARCHDUMP_OUTPUT_*` environment variables or
//! `--httpAuthFile`; children inherit them. `user:pass@` in a URL works but puts the password on
//! every child's command line (visible in `ps`). When `--httpAuthFile` is given it wins over URL
//! userinfo and env (L-645b).
//!
//! TLS (1.4, L4-001): `--ca`, `--cert`/`--key` and `--insecure` apply to the cluster and go to
//! every child as `--input-*` (dump) or `--output-*` (load). The key passphrase (`--pass`, or
//! `$SEARCHDUMP_TLS_PASS`) reaches children only through `$SEARCHDUMP_{INPUT,OUTPUT}_TLS_PASS`,
//! never their command line. Per-child ids (L4-002): every child run gets a UUID in
//! `$SEARCHDUMP_CHILD_ID`, printed by the parent, tagged on the child's log lines and recorded in
//! its stats, the manifest and `LOGDIR/children.json`.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, ValueEnum};
use http::Method;
use regex::Regex;
use searchdump_rs::endpoint::read_auth_file;
use searchdump_rs::http::gate::RequestGate;
use searchdump_rs::http::retry::error_chain;
use searchdump_rs::http::tls::TlsOptions;
use searchdump_rs::http::{Body, ClientOptions, EsClient};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

// split (L4-S03..): one module per concern; the glob imports are narrowed in step 15
mod child;
mod cli;
mod cluster;
mod discover;
mod files;
mod flags;
mod manifest;
mod selection;
pub mod sizing;
mod tls;
mod types;
use child::*;
use cli::*;
use cluster::*;
use discover::*;
use files::*;
use flags::*;
use manifest::*;
use selection::*;
use tls::*;
use types::*;

/// 1.2: the per-index note when an index gets a page below the 1.1 default (large documents
/// or the budget), and a warning when the user's `-- --limit` is above the safe page.
fn note_size(ctx: &Ctx, index: &str, st: Option<sizing::Stats>, size: &sizing::IndexSize) {
    let user_limit = ctx.args.extra.iter().find_map(|a| {
        a.strip_prefix("--limit=")
            .and_then(|v| v.parse::<u64>().ok())
    });
    let avg_kb = st
        .and_then(|(d, b)| (d > 0).then(|| (b / d).div_ceil(1024)))
        .unwrap_or(0);
    match user_limit {
        Some(u) if ctx.plan().budget.is_some() && u > size.limit => eprintln!(
            "warning: {index}: -- --limit={u} is above the page the cluster's limits allow ({} documents of ~{avg_kb} KB stored); kept because you chose it",
            size.limit
        ),
        Some(_) => {}
        None if ctx.plan().no_defaults => {}
        None if size.limit < sizing::OFF_LIMIT => eprintln!(
            "note: {index}: documents average {avg_kb} KB stored; dumping {} per page instead of {} ({}; give -- --limit=N to choose)",
            size.limit,
            if ctx.plan().budget.is_some() {
                limits_max_page()
            } else {
                sizing::OFF_LIMIT
            },
            size.reason
        ),
        None => {}
    }
}

fn limits_max_page() -> u64 {
    searchdump_rs::limits::MULTI_MAX_PAGE
}

/// The manifest's per-index `sizing` object.
fn sizing_json(size: &sizing::IndexSize, user: &[String]) -> serde_json::Value {
    let user_limit = user.iter().any(|a| flag_name(a) == Some("--limit"));
    serde_json::json!({
        "limit": size.limit,
        "max_bulk_bytes": size.max_bulk_bytes,
        "reason": if user_limit { "user" } else { size.reason },
    })
}

/// 1.1-7: `SEARCHDUMP_MULTI_DEBUG=1` prints where each record count came from.
fn debug_counts() -> bool {
    std::env::var_os("SEARCHDUMP_MULTI_DEBUG").is_some_and(|v| v != "0" && !v.is_empty())
}

// ----------------------------------------------------------------------------- dump / load

#[derive(Debug)]
pub enum Outcome {
    Done(String),
    Skipped(String),
    Failed(String),
}

struct Ctx {
    args: Args,
    /// L4-001: TLS settings of the cluster (parent requests; children get them as flags/env).
    tls: ClusterTls,
    /// Where the manifest and `_cluster/` live (the dump directory unless --metaDir or
    /// --nodeCompat put them elsewhere).
    meta: PathBuf,
    types: Types,
    sel: Selector,
    runner: Runner,
    pacer: Pacer,
    /// 1.2: the sizing plan (set by [`plan_sizes`] before any index starts).
    plan: std::sync::OnceLock<sizing::Plan>,
    /// 1.2: primary (docs, stored bytes) per selected index, from one stats call.
    index_stats: std::sync::OnceLock<BTreeMap<String, sizing::Stats>>,
}

impl Ctx {
    fn plan(&self) -> &sizing::Plan {
        self.plan.get_or_init(|| {
            sizing::Plan::new(
                searchdump_rs::limits::Mode::Off,
                self.args.no_default_data_flags,
                None,
                self.args.parallel,
                &[],
            )
        })
    }

    fn stats_of(&self, index: &str) -> Option<sizing::Stats> {
        self.index_stats.get().and_then(|m| m.get(index).copied())
    }
}

/// 1.2: probe the cluster once, read every target's stats in one call, and set the plan
/// (ROADMAP-1.2 section 4). `targets` are the indices to dump (empty for a load). Returns the
/// exit code to stop with for `--serverLimits=check` (0, or 2 when a flag after `--` exceeds
/// the limits) and the manifest's `server_limits` object.
async fn plan_sizes(
    ctx: &Ctx,
    cluster: &Cluster,
    targets: &[String],
) -> (Option<i32>, serde_json::Value) {
    use searchdump_rs::limits::{self, Mode};
    let no_defaults = ctx.args.no_default_data_flags && ctx.args.server_limits.is_none();
    let mode = ctx.args.server_limits.unwrap_or(if no_defaults {
        Mode::Off
    } else {
        limits::DEFAULT_MODE
    });
    let probed = if mode == Mode::Off {
        None
    } else {
        Some(cluster.probe_limits().await)
    };
    let user_limit = ctx
        .args
        .extra
        .iter()
        .any(|a| flag_name(a) == Some("--limit"));
    let mut stats = BTreeMap::new();
    let skip_stats = targets.is_empty() || no_defaults || (mode == Mode::Off && user_limit);
    if !skip_stats {
        match cluster.index_stats().await {
            Ok(m) => stats = m,
            Err(e) => eprintln!("note: index stats in one call failed ({e:#}); asking per index"),
        }
        for t in targets {
            if !stats.contains_key(t)
                && let Some(s) = cluster.target_stats(t).await
            {
                stats.insert(t.clone(), s);
            }
        }
    }
    let per: Vec<Option<sizing::Stats>> = targets.iter().map(|t| stats.get(t).copied()).collect();
    let plan = sizing::Plan::new(mode, no_defaults, probed, ctx.args.parallel, &per)
        .with_page_bytes(ctx.args.page_bytes);
    eprintln!("{}", plan.line());
    let json = match &plan.limits {
        Some(l) => limits::to_json(l, mode, plan.budget.as_ref()),
        None => serde_json::json!({ "mode": mode.as_str(), "source": "off" }),
    };
    let mut json = json;
    if let Some(o) = json.as_object_mut() {
        o.insert("parallel".into(), ctx.args.parallel.into());
        o.insert("fit_parallel".into(), plan.fit_parallel.into());
        o.insert("bulk_bytes_per_worker".into(), plan.bulk_bytes.into());
    }
    let mut stop = None;
    if mode == Mode::Check {
        let mut too_big = vec![];
        let user_limit_v = ctx.args.extra.iter().find_map(|a| {
            a.strip_prefix("--limit=")
                .and_then(|v| v.parse::<u64>().ok())
        });
        let user_bulk = ctx.args.extra.iter().find_map(|a| {
            a.strip_prefix("--maxBulkBytes=")
                .and_then(|v| searchdump_rs::pipeline::file_sink::parse_size(v).ok())
        });
        if targets.is_empty() {
            let l = plan.load();
            println!(
                "load: --limit={} --maxBulkBytes={} KB per worker ({}), parallel {}",
                l.limit,
                l.max_bulk_bytes >> 10,
                l.reason,
                ctx.args.parallel
            );
            if plan.budget.is_some()
                && let Some(u) = user_bulk
                && u > l.max_bulk_bytes
            {
                too_big.push(format!("--maxBulkBytes={u} > {}", l.max_bulk_bytes));
            }
        }
        for (t, st) in targets.iter().zip(&per) {
            let d = plan.dump(*st);
            let avg = st.and_then(|(n, b)| (n > 0).then(|| b / n)).unwrap_or(0);
            println!(
                "  {t}: --limit={} ({}, ~{} KB stored per document, ~{} MB per page)",
                d.limit,
                d.reason,
                avg.div_ceil(1024),
                d.page_bytes >> 20
            );
            if plan.budget.is_some()
                && let Some(u) = user_limit_v
                && u > d.limit
            {
                too_big.push(format!("{t}: --limit={u} > {}", d.limit));
            }
        }
        if too_big.is_empty() {
            println!("server limits: check passed");
            stop = Some(0);
        } else {
            eprintln!(
                "server limits: flags after -- exceed the limits: {}",
                too_big.join("; ")
            );
            stop = Some(2);
        }
    }
    let _ = ctx.plan.set(plan);
    let _ = ctx.index_stats.set(stats);
    (stop, json)
}

/// `--interval`: a minimum gap between job starts, shared by all workers.
pub struct Pacer {
    gap: Duration,
    next: tokio::sync::Mutex<Option<tokio::time::Instant>>,
}

impl Pacer {
    pub fn new(gap: Duration) -> Self {
        Self {
            gap,
            next: tokio::sync::Mutex::new(None),
        }
    }

    pub async fn wait(&self) {
        if self.gap.is_zero() {
            return;
        }
        let mut next = self.next.lock().await;
        if let Some(at) = *next {
            tokio::time::sleep_until(at).await;
        }
        *next = Some(tokio::time::Instant::now() + self.gap);
    }
}

async fn dump_one(
    ctx: &Ctx,
    cluster: &Cluster,
    root: &Path,
    manifest: &Mutex<Manifest>,
    target: &Target,
) -> Outcome {
    ctx.pacer.wait().await;
    let index = target.name.as_str();
    let ds = target.kind == TargetKind::DataStream;
    let kinds: Vec<&str> = ctx
        .types
        .index
        .iter()
        .copied()
        .filter(|k| !ds || *k == "data")
        .collect();
    if kinds.is_empty() {
        return Outcome::Skipped(
            "data stream: only its data is dumped (its settings and mappings come from its index template); add data to --types".into(),
        );
    }
    let rel = match rel_dir(ctx.args.layout, &ctx.sel, index, &target.matched) {
        Ok(r) => r,
        Err(e) => return Outcome::Failed(format!("{e:#}")),
    };
    let dir = root.join(&rel);
    let comp = ctx.args.compression;
    let split = ctx.args.splits();
    let node = ctx.args.node_compat;
    let mut entry = IndexEntry {
        dir: rel.to_string_lossy().into_owned(),
        kind: ds.then(|| "datastream".to_string()),
        group: (ctx.args.layout == Layout::Group).then(|| rel.to_string_lossy().into_owned()),
        types: kinds.iter().map(|t| t.to_string()).collect(),
        status: "running".into(),
        ..Default::default()
    };
    let res: Result<String> = async {
        // 1.1-7: what the data run reported (its record count, unless --verifyCounts)
        let mut data_stats: Option<ChildStats> = None;
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let source = cluster.index_url(index);
        entry.source_count = cluster.count(index, false).await.ok();
        if ds {
            let info = cluster
                .data_stream(index)
                .await?
                .ok_or_else(|| anyhow!("data stream {index} disappeared"))?;
            let marker = dir.join(format!("{index}.datastream.json"));
            fs::write(&marker, serde_json::to_vec_pretty(&info)?)
                .with_context(|| format!("write {}", marker.display()))?;
        }
        for kind in &kinds {
            let (out, extra) = if *kind == "data" {
                remove_data_files(&dir, index)?;
                let mut extra = strings(comp.flags());
                if let Some(s) = &ctx.args.file_size {
                    extra.push(format!("--fileSize={s}"));
                }
                if let Some(n) = ctx.args.max_rows {
                    extra.push(format!("--maxRows={n}"));
                }
                // L4-003: the child keeps the --pageBytes cap at run time (a page that comes
                // back bigger than the estimate shrinks the next ones); Node cannot
                if let Some(p) = ctx.args.page_bytes
                    && ctx.runner.server_limits
                {
                    extra.push(format!("--pageBytes={p}"));
                }
                // 1.2: this index's sizes from the parent's plan (one stats call for all
                // indices, no per-index probe); the user's flags after `--` still win.
                let st = ctx.stats_of(index);
                let size = ctx.plan().dump(st);
                note_size(ctx, index, st, &size);
                entry.sizing = Some(sizing_json(&size, &ctx.args.extra));
                extra.extend(merge_data_flags_owned(
                    &ctx.plan().flags(&size, ctx.runner.server_limits),
                    &ctx.runner.data_flags,
                ));
                // searchdump adds the codec extension to each part itself.
                let name = if split {
                    item_file_for(node, index, "data", Compression::None)
                } else {
                    item_file_for(node, index, "data", comp)
                };
                (dir.join(name), extra)
            } else if node {
                // Node gzips every file with --fsCompress, not only the data.
                (
                    dir.join(node_item_file(index, kind, comp)),
                    strings(comp.flags()),
                )
            } else {
                (dir.join(item_file(index, kind, comp)), vec![])
            };
            let o = out.to_string_lossy().into_owned();
            // Node's per-index template file holds the cluster's legacy templates.
            let input = if *kind == "template" {
                cluster.child_base.clone()
            } else {
                source.clone()
            };
            let runner = &ctx.runner;
            let stats = runner.run(index, kind, &input, &o, &extra).await?;
            if *kind == "data" {
                data_stats = Some(stats);
            }
            if !matches!(*kind, "data" | "analyzer" | "template") && !nonempty(&out) {
                bail!("{kind} export is empty");
            }
        }
        let mut summary = String::new();
        if kinds.contains(&"data") {
            let files: Vec<PathBuf> = if split {
                data_parts(&dir, index)?
            } else {
                let f = dir.join(item_file_for(node, index, "data", comp));
                if f.exists() { vec![f] } else { vec![] }
            };
            let reported = data_stats.and_then(|s| s.written.zip(s.source));
            let records = match reported {
                // nothing written (no data file) needs no reading either
                _ if files.is_empty() => 0,
                Some((n, source)) if !ctx.args.verify_counts => {
                    if debug_counts() {
                        eprintln!("debug: {index}: {n} records ({source:?})");
                    }
                    n
                }
                _ => {
                    let mut scanned = 0;
                    for f in &files {
                        scanned += count_records(f)?;
                    }
                    if let Some((n, source)) = reported
                        && n != scanned
                    {
                        bail!(
                            "the data files hold {scanned} records but the searchdump run reported {n} ({source:?})"
                        );
                    }
                    if debug_counts() {
                        eprintln!("debug: {index}: {scanned} records (read back)");
                    }
                    scanned
                }
            };
            if split {
                entry.data_parts = files.iter().map(|f| file_name(f)).collect();
            } else {
                entry.data_file = Some(item_file_for(node, index, "data", comp));
            }
            entry.records = Some(records);
            if let Some(src) = entry.source_count {
                if records == 0 && src > 0 {
                    bail!("data export is empty but the index has {src} documents");
                }
                if records != src {
                    eprintln!("warning: {index}: {records} records written, the index reported {src} before the dump (still being written to?)");
                }
            }
            summary = if split {
                format!(" ({records} records in {} parts)", files.len())
            } else {
                format!(" ({records} records)")
            };
        }
        if ds {
            summary.push_str(" [data stream]");
        }
        Ok(format!("{}{summary}", dir.display()))
    }
    .await;
    let outcome = match res {
        Ok(s) => {
            entry.status = "done".into();
            Outcome::Done(s)
        }
        Err(e) => {
            entry.status = "failed".into();
            entry.error = Some(format!("{e:#}"));
            Outcome::Failed(format!("{e:#}"))
        }
    };
    entry.children = ctx.runner.refs_for(index);
    let mut m = manifest.lock().unwrap();
    m.indices.insert(index.to_string(), entry);
    if let Err(e) = m.save(&ctx.meta) {
        eprintln!("warning: could not write the manifest: {e:#}");
    }
    outcome
}

/// Dump every selected cluster-scoped type once, to `ROOT/_cluster/TYPE.json`.
async fn dump_cluster(
    ctx: &Ctx,
    cluster: &Cluster,
    manifest: &Mutex<Manifest>,
) -> Vec<(String, Outcome)> {
    let dir = ctx.meta.join(CLUSTER_DIR);
    let mut out = vec![];
    for kind in &ctx.types.cluster {
        let name = format!("{CLUSTER_DIR}/{kind}");
        let file = dir.join(format!("{kind}.json"));
        let res =
            match fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display())) {
                Ok(()) => {
                    let o = file.to_string_lossy().into_owned();
                    ctx.runner
                        .run(CLUSTER_DIR, kind, &cluster.child_base, &o, &[])
                        .await
                }
                Err(e) => Err(e),
            };
        let entry = ClusterEntry {
            file: format!("{kind}.json"),
            status: if res.is_ok() { "done" } else { "failed" }.into(),
            error: res.as_ref().err().map(|e| format!("{e:#}")),
        };
        out.push((
            name,
            match res {
                Ok(_) => Outcome::Done(file.display().to_string()),
                Err(e) => Outcome::Failed(format!("{e:#}")),
            },
        ));
        let mut m = manifest.lock().unwrap();
        m.cluster.insert(kind.to_string(), entry);
        if let Err(e) = m.save(&ctx.meta) {
            eprintln!("warning: could not write the manifest: {e:#}");
        }
    }
    out
}

/// Load the cluster-scoped files present under `ROOT/_cluster`, in CLUSTER_TYPES order.
async fn load_cluster(ctx: &Ctx, cluster: &Cluster) -> Vec<(String, Outcome)> {
    let dir = ctx.meta.join(CLUSTER_DIR);
    let recorded = Manifest::load(&ctx.meta).ok().flatten().map(|m| m.cluster);
    let mut out = vec![];
    for kind in &ctx.types.cluster {
        let name = format!("{CLUSTER_DIR}/{kind}");
        if recorded
            .as_ref()
            .and_then(|c| c.get(*kind))
            .is_some_and(|e| e.status != "done")
        {
            out.push((
                name,
                Outcome::Skipped("recorded as failed in the manifest".into()),
            ));
            continue;
        }
        let file = dir.join(format!("{kind}.json"));
        if !nonempty(&file) {
            if !ctx.args.types.is_empty() {
                out.push((name, Outcome::Skipped(format!("no {}", file.display()))));
            }
            continue;
        }
        let input = file.to_string_lossy().into_owned();
        let extra = cluster_flags(&ctx.args, kind, true);
        let r = ctx
            .runner
            .run(CLUSTER_DIR, kind, &input, &cluster.child_base, &extra)
            .await;
        out.push((
            name,
            match r {
                Ok(_) => Outcome::Done(format!("from {}", file.display())),
                Err(e) => Outcome::Failed(format!("{e:#}")),
            },
        ));
    }
    out
}

async fn load_one(ctx: &Ctx, cluster: &Cluster, f: &Found) -> Outcome {
    ctx.pacer.wait().await;
    let target = format!("{}{}{}", ctx.args.prefix, f.index, ctx.args.suffix);
    let res: Result<Option<String>> = async {
        check_index_name(&target)?;
        if cluster.exists(&target).await? {
            if ctx.args.skip_existing {
                return Ok(None);
            }
            bail!("{target} already exists on the target (use --skipExisting to skip it)");
        }
        let url = cluster.index_url(&target);
        let mut records = None;
        let mut settings_applied = false;
        let kinds: Vec<&str> = ctx
            .types
            .index
            .iter()
            .copied()
            .filter(|k| !f.datastream || *k == "data")
            .collect();
        // A Node-layout dump carries the legacy templates next to each index; load them
        // first, as Node does (the same templates again for each index is harmless).
        if ctx.types.cluster.contains(&"template")
            && !f.datastream
            && let Some((p, gz)) = meta_file(&f.dir, &f.index, "template")
        {
            let mut extra = cluster_flags(&ctx.args, "template", true);
            if gz {
                extra.push("--fsCompress".into());
            }
            let input = p.to_string_lossy().into_owned();
            let runner = &ctx.runner;
            runner
                .run(&f.index, "template", &input, &cluster.child_base, &extra)
                .await?;
        }
        for kind in kinds {
            if kind == "data" {
                if f.data.is_empty() {
                    let meta_only =
                        f.datastream || meta_file(&f.dir, &f.index, "settings").is_some();
                    if f.records_hint.unwrap_or(0) == 0 && meta_only {
                        eprintln!(
                            "note: {}: no data file (empty index); loading metadata only",
                            f.index
                        );
                        continue;
                    }
                    bail!("missing data file for {} in {}", f.index, f.dir.display());
                }
                let mut total = 0;
                for (n, part) in f.data.iter().enumerate() {
                    let c = parse_data_file(&file_name(part)).map_or(Compression::None, |d| d.1);
                    let mut extra = strings(c.flags());
                    let size = ctx.plan().load();
                    extra.extend(merge_data_flags_owned(
                        &ctx.plan().flags(&size, ctx.runner.server_limits),
                        &ctx.runner.data_flags,
                    ));
                    if f.datastream && !extra.iter().any(|a| a.starts_with("--bulkAction")) {
                        // A data stream only accepts create.
                        extra.push("--bulkAction=create".into());
                    }
                    let label = if f.data.len() > 1 {
                        format!("data.part-{n}")
                    } else {
                        "data".into()
                    };
                    let input = part.to_string_lossy().into_owned();
                    let runner = &ctx.runner;
                    let stats = runner
                        .run_as(&f.index, "data", &label, &input, &url, &extra)
                        .await
                        .map_err(|e| with_part(e, f.data.len(), n, part))?;
                    // 1.1-7: the documents the run read (written + failed), else read the part
                    total += match stats.docs_read() {
                        Some(read) if !ctx.args.verify_counts => read,
                        reported => {
                            let scanned = count_records(part)?;
                            if let Some(read) = reported
                                && read != scanned
                            {
                                bail!(
                                    "{} holds {scanned} records but the searchdump run read {read}",
                                    part.display()
                                );
                            }
                            scanned
                        }
                    };
                }
                records = Some(total);
                continue;
            }
            if kind == "analyzer" && settings_applied {
                // The settings file already carries index.analysis.
                continue;
            }
            let Some((p, gz)) = meta_file(&f.dir, &f.index, kind) else {
                if matches!(kind, "alias" | "analyzer") {
                    continue;
                }
                bail!(
                    "missing {kind} file {}",
                    f.dir
                        .join(item_file(&f.index, kind, Compression::None))
                        .display()
                );
            };
            let mut extra = if kind == "settings" {
                match ctx.args.lifecycle {
                    Lifecycle::Drop => vec![LIFECYCLE_DROP.to_string()],
                    Lifecycle::Convert => vec!["--convertIlmToIsm".into()],
                }
            } else {
                vec![]
            };
            if gz {
                extra.push("--fsCompress".into());
            }
            let input = p.to_string_lossy().into_owned();
            let runner = &ctx.runner;
            let r = runner.run(&f.index, kind, &input, &url, &extra).await;
            match r {
                Ok(_) => settings_applied |= kind == "settings",
                Err(e) if kind == "mapping" => {
                    eprintln!(
                        "note: {target}: mapping via searchdump failed, applying it directly"
                    );
                    let text = read_text(&p)?;
                    cluster
                        .put_mapping(&target, &mappings_body(&text)?)
                        .await
                        .with_context(|| format!("{e:#}"))?;
                }
                Err(e) => return Err(e),
            }
        }
        if let Some(want) = records {
            let got = cluster.count(&target, true).await?;
            if got != want {
                bail!("restored {target} has {got} documents, the backup has {want} records");
            }
            let parts = if f.data.len() > 1 {
                format!(" from {} parts", f.data.len())
            } else {
                String::new()
            };
            return Ok(Some(format!("{target} docs={got}{parts}")));
        }
        Ok(Some(target.clone()))
    }
    .await;
    match res {
        Ok(Some(s)) => Outcome::Done(s),
        Ok(None) => Outcome::Skipped(format!("{target} already exists")),
        Err(e) => Outcome::Failed(format!("{e:#}")),
    }
}

/// The part a failed data load step was reading, when the index has more than one. A single data
/// file adds no context: an empty one printed as "FAILED  IX: : interrupted: ..." (1.1-item-checks,
/// check 4).
fn with_part(e: anyhow::Error, parts: usize, n: usize, part: &Path) -> anyhow::Error {
    if parts > 1 {
        e.context(format!("part {n} ({})", part.display()))
    } else {
        e
    }
}

/// L-809a: storage-URL directories. `copy` copies a dump directory between stores; a dump to
/// a storage URL is written to a local staging directory and uploaded when every index
/// succeeded; a load from one is downloaded to a staging directory first.
#[cfg_attr(not(feature = "opendal"), allow(unused_variables, unused_mut))]
async fn run(mut args: Args) -> Result<i32> {
    use searchdump_rs::remote;
    if args.parallel == 0 {
        bail!("--parallel must be at least 1");
    }
    let in_cfg = args.input_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_INPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let out_cfg = args.output_storage_config.clone().or_else(|| {
        searchdump_rs::envnames::var_os("SEARCHDUMP_OUTPUT_STORAGE_CONFIG").map(PathBuf::from)
    });
    let is_cluster = |u: &str| {
        let l = u.to_ascii_lowercase();
        l.starts_with("http://") || l.starts_with("https://")
    };
    match args.direction {
        Direction::Copy => {
            if is_cluster(&args.input) || is_cluster(&args.output) {
                bail!(
                    "--direction copy copies a dump directory between stores; use dump or load for a cluster"
                );
            }
            if ClusterTls::any_flag(&args) {
                bail!(
                    "--direction copy has no cluster, so --ca/--cert/--key/--pass do not apply; set ca_file= or insecure= in the storage config for an s3:// or b2:// endpoint"
                );
            }
            if args.node_compat
                || args.file_size.is_some()
                || args.max_rows.is_some()
                || args.page_bytes.is_some()
            {
                bail!(
                    "--direction copy copies the files as they are (format, compression and split parts kept); --nodeCompat, --fileSize, --maxRows and --pageBytes do not apply"
                );
            }
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                if src == dst {
                    bail!("--input and --output are the same directory");
                }
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel).await?;
                println!(
                    "copied {} files ({} bytes) from {src} to {dst}",
                    r.files, r.bytes
                );
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("--direction copy needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.output) => {
            #[cfg(feature = "opendal")]
            {
                let dst = remote::parse_dir(&args.output, out_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                args.output = staging.display().to_string();
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --output");
                }
                if args.dry_run {
                    return run_local(args).await;
                }
                eprintln!(
                    "dumping to the staging directory {} first",
                    staging.display()
                );
                let code = run_local(args.clone()).await?;
                if code != 0 {
                    eprintln!(
                        "not uploaded to {dst}: some steps failed. The dump is kept in {}; finish it with --stagingDir {} --resume (same arguments), which uploads it",
                        staging.display(),
                        staging.display()
                    );
                    return Ok(code);
                }
                let src = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, args.overwrite, args.parallel)
                    .await
                    .with_context(|| {
                        format!(
                            "uploading the dump failed; it is kept in {} (rerun with --stagingDir {} --resume, or copy it with --direction copy)",
                            staging.display(),
                            staging.display()
                        )
                    })?;
                println!("uploaded {} files ({} bytes) to {dst}", r.files, r.bytes);
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                } else {
                    eprintln!("the staged copy stays in {}", staging.display());
                }
                Ok(0)
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --output needs a build with the `opendal` feature");
        }
        Direction::Load if remote::is_storage_url(&args.input) => {
            #[cfg(feature = "opendal")]
            {
                let src = remote::parse_dir(&args.input, in_cfg.as_deref())?;
                let (staging, temp) = staging_dir(&args)?;
                if args.meta_dir.is_some() {
                    bail!("--metaDir cannot be combined with a storage-URL --input");
                }
                let dst = remote::parse_dir(&staging.display().to_string(), None)?;
                let r = remote::copy_tree(&src, &dst, true, args.parallel).await?;
                eprintln!(
                    "downloaded {} files ({} bytes) from {src} to {}",
                    r.files,
                    r.bytes,
                    staging.display()
                );
                args.input = staging.display().to_string();
                let code = run_local(args).await;
                if temp {
                    let _ = fs::remove_dir_all(&staging);
                }
                code
            }
            #[cfg(not(feature = "opendal"))]
            bail!("a storage-URL --input needs a build with the `opendal` feature");
        }
        Direction::Dump if remote::is_storage_url(&args.input) => bail!(
            "--direction dump reads a cluster; to copy a dump directory between stores use --direction copy"
        ),
        Direction::Load if remote::is_storage_url(&args.output) => bail!(
            "--direction load writes into a cluster; to copy a dump directory between stores use --direction copy"
        ),
        _ => run_local(args).await,
    }
}

/// `--stagingDir`, or a new temp directory (`true`: made here, removed when done).
#[cfg(feature = "opendal")]
fn staging_dir(args: &Args) -> Result<(PathBuf, bool)> {
    match &args.staging_dir {
        Some(d) => {
            fs::create_dir_all(d).with_context(|| format!("create {}", d.display()))?;
            Ok((d.clone(), false))
        }
        None => {
            let d = std::env::temp_dir().join(format!(
                "searchdump-multi-staging-{}-{}",
                std::process::id(),
                chrono_stamp()
            ));
            fs::create_dir_all(&d).with_context(|| format!("create {}", d.display()))?;
            Ok((d, true))
        }
    }
}

async fn run_local(args: Args) -> Result<i32> {
    if args.direction == Direction::Load && (args.file_size.is_some() || args.max_rows.is_some()) {
        bail!("--fileSize/--maxRows apply to dumps; load finds split parts on its own");
    }
    if args.direction != Direction::Dump && args.page_bytes.is_some() {
        bail!("--pageBytes applies to dumps (it caps the search pages read from the cluster)");
    }
    if args.node_compat {
        if args.direction == Direction::Load {
            bail!("--nodeCompat applies to dumps; load reads Node-named directories on its own");
        }
        if args.layout != Layout::Flat {
            bail!("--nodeCompat needs --layout flat (Node reads one directory, no subdirectories)");
        }
        if !matches!(args.compression, Compression::Gzip | Compression::None) {
            bail!("--nodeCompat: Node reads gzip or uncompressed files only, not zstd");
        }
        if args.splits() {
            bail!(
                "--nodeCompat cannot split data: Node multielasticdump 6.x cannot load split parts (it opens INDEX.json or 'undefined' for them); dump without --nodeCompat, or without --fileSize/--maxRows"
            );
        }
        if args.match_type.contains(&MatchType::Datastream) {
            bail!(
                "--nodeCompat: Node multielasticdump cannot restore data streams (its data runs use the index op); dump them without --nodeCompat"
            );
        }
    }
    let mut default_types: Vec<String> = match args.direction {
        // Node dumps (and by default loads) the legacy templates with every index.
        Direction::Dump if args.node_compat => vec!["settings", "mapping", "template", "data"],
        Direction::Dump => vec!["settings", "mapping", "data"],
        Direction::Load => vec!["settings", "analyzer", "mapping", "alias", "data"],
        Direction::Copy => unreachable!("copy is handled before run_local"),
    }
    .into_iter()
    .map(String::from)
    .collect();
    if args.direction == Direction::Load {
        default_types.extend(CLUSTER_TYPES.iter().map(|s| s.to_string()));
    }
    let (want, ignore) = node_type_flags(&args, &default_types);
    let types = resolve_types(&want, &ignore)?;
    let mut types = types;
    if args.node_compat {
        if let Some(i) = types.cluster.iter().position(|k| *k == "template") {
            // Node keeps the legacy templates in INDEX.template.json, loaded before the index.
            types.cluster.remove(i);
            types.index.insert(0, "template");
        } else {
            eprintln!(
                "warning: no template type: Node's multielasticdump load stops on the missing INDEX.template.json unless run with --ignoreTemplate=true"
            );
        }
        for k in ["settings", "mapping", "data"] {
            if !types.index.contains(&k) {
                eprintln!(
                    "warning: no {k} type: Node's multielasticdump load stops on the missing files unless run with --ignore{}=true",
                    match k {
                        "settings" => "Settings",
                        "mapping" => "Mapping",
                        _ => "Data",
                    }
                );
            }
        }
    }
    let sel = Selector::new(&args.match_re, args.exclude.as_deref(), args.include_hidden)?;
    if args.layout == Layout::Group && !sel.has_group() {
        bail!("--layout group needs a capture group in --match, e.g. '^logs-(?P<group>[a-z]+)-'");
    }
    let creds_in = |u: &str| {
        Url::parse(u)
            .map(|u| u.password().is_some())
            .unwrap_or(false)
    };
    if creds_in(&args.input) || creds_in(&args.output) {
        eprintln!(
            "warning: a password in the URL is passed to every searchdump run and shows in `ps`; prefer SEARCHDUMP_*_PASSWORD or --httpAuthFile"
        );
    }
    let log_dir = match &args.log_dir {
        Some(d) => d.clone(),
        None => std::env::temp_dir().join(format!(
            "searchdump-multi-{}-{}",
            std::process::id(),
            chrono_stamp()
        )),
    };
    // L4-001: the cluster's TLS settings, for the parent and every child
    let side = if args.direction == Direction::Dump {
        Side::Input
    } else {
        Side::Output
    };
    let tls = ClusterTls::resolve(&args, side, &|k| std::env::var(k).ok())?;
    if tls.node_insecure {
        // the same words searchdump prints
        eprintln!(
            "warning: NODE_TLS_REJECT_UNAUTHORIZED=0: TLS certificate verification is disabled"
        );
    }
    if let Some(p) = &tls.pass {
        searchdump_rs::secrets::register(&p.0);
    }
    let mut common = tls.child_flags(side);
    if let Some(f) = &args.http_auth_file {
        common.push(format!("--httpAuthFile={f}"));
    }
    let data_flags = merge_data_flags(
        if args.no_default_data_flags {
            &[]
        } else {
            DATA_FLAGS
        },
        &args.extra,
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel = CancellationToken::new();
    {
        let c = cancelled.clone();
        let token = cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!(
                    "interrupted: stopping running steps (SIGTERM, SIGKILL after {}s), starting no new ones",
                    CHILD_GRACE.as_secs()
                );
                c.store(true, Ordering::SeqCst);
                token.cancel();
            }
        });
    }
    let bin = find_bin(args.elasticdump_bin.as_deref());
    let stats = takes_stats_file(&bin) && accepts_stats_file(&bin);
    let server_limits = takes_stats_file(&bin) && accepts_server_limits(&bin);
    if tls.pass.is_some() && !takes_stats_file(&bin) {
        bail!(
            "a key passphrase is passed to searchdump through ${}, which {} does not read; use searchdump, or an unencrypted key",
            searchdump_rs::config::tls_pass_env(side),
            bin.display()
        );
    }
    if args.page_bytes.is_some() && !server_limits {
        eprintln!(
            "warning: {} does not take --pageBytes; it only sizes the pages from the index statistics",
            bin.display()
        );
    }
    let (child_env, child_env_remove) = tls.child_env(side);
    let runner = Runner {
        runs: Mutex::new(vec![]),
        child_env,
        child_env_remove,
        bin,
        log_dir: log_dir.clone(),
        common,
        data_flags,
        cancelled: cancelled.clone(),
        cancel: cancel.clone(),
        grace: CHILD_GRACE,
        stats,
        server_limits,
    };
    let pacer = Pacer::new(Duration::from_millis(args.interval));
    let meta = meta_dir(&args);
    let ctx = Arc::new(Ctx {
        meta,
        tls,
        args,
        types,
        sel,
        runner,
        pacer,
        plan: Default::default(),
        index_stats: Default::default(),
    });
    let started = Instant::now();
    let sem = Arc::new(tokio::sync::Semaphore::new(ctx.args.parallel));
    let mut set = tokio::task::JoinSet::new();
    // Cluster-scoped results, reported with the per-index ones.
    let mut early: Vec<(String, Outcome)> = vec![];

    match ctx.args.direction {
        Direction::Dump => {
            let cluster = Arc::new(Cluster::new(
                &ctx.args.input,
                "SEARCHDUMP_INPUT",
                &ctx.args,
                &ctx.tls,
            )?);
            let mt = &ctx.args.match_type;
            let mut selected = vec![];
            let mut total = 0;
            if !ctx.types.index.is_empty() {
                let indices = cluster.list_indices().await?;
                let aliases = if mt.contains(&MatchType::Alias) {
                    cluster.list_aliases().await?
                } else {
                    vec![]
                };
                let streams = if mt.contains(&MatchType::Datastream) {
                    cluster.list_data_streams().await?
                } else {
                    vec![]
                };
                total = indices.len() + streams.len();
                selected = select_targets(&ctx.sel, mt, &indices, &aliases, &streams);
                if ctx.args.order == Order::Desc {
                    selected.reverse();
                }
                for t in &selected {
                    check_index_name(&t.name)?;
                }
                if selected.iter().any(|t| t.kind == TargetKind::DataStream)
                    && !ctx.types.cluster.contains(&"index_template")
                {
                    eprintln!(
                        "warning: a data stream loads only where a matching index template exists; add index_template (and component_template) to --types to dump them too"
                    );
                }
            }
            let root = PathBuf::from(&ctx.args.output);
            if ctx.args.dry_run {
                if !ctx.types.cluster.is_empty() {
                    println!("cluster-wide:");
                    for k in &ctx.types.cluster {
                        println!(
                            "  {}",
                            ctx.meta
                                .join(CLUSTER_DIR)
                                .join(format!("{k}.json"))
                                .display()
                        );
                    }
                }
                println!(
                    "{} of {total} indices and data streams selected:",
                    selected.len()
                );
                let split = ctx.args.splits();
                for t in &selected {
                    let rel = rel_dir(ctx.args.layout, &ctx.sel, &t.name, &t.matched)?;
                    let d = root.join(&rel);
                    let mut files: Vec<String> = vec![];
                    if t.kind == TargetKind::DataStream {
                        files.push(
                            d.join(format!("{}.datastream.json", t.name))
                                .display()
                                .to_string(),
                        );
                    }
                    for k in ctx
                        .types
                        .index
                        .iter()
                        .filter(|k| t.kind == TargetKind::Index || **k == "data")
                    {
                        let node = ctx.args.node_compat;
                        let f = if *k == "data" && split {
                            if node {
                                node_item_file(
                                    &format!("{}.split-N", t.name),
                                    k,
                                    ctx.args.compression,
                                )
                            } else {
                                format!("{}.data.split-N.{}", t.name, ctx.args.compression.suffix())
                            }
                        } else {
                            item_file_for(node, &t.name, k, ctx.args.compression)
                        };
                        files.push(d.join(f).display().to_string());
                    }
                    let via = if t.matched != t.name {
                        format!(" (via alias {})", t.matched)
                    } else {
                        String::new()
                    };
                    let ds = if t.kind == TargetKind::DataStream {
                        " [data stream]"
                    } else {
                        ""
                    };
                    println!("  {}{ds}{via}\n    {}", t.name, files.join("\n    "));
                }
                return Ok(0);
            }
            if selected.is_empty() && ctx.types.cluster.is_empty() {
                eprintln!(
                    "nothing matches --match '{}' (of {total} indices and data streams)",
                    ctx.args.match_re,
                );
                return Ok(1);
            }
            let names: Vec<String> = selected.iter().map(|t| t.name.clone()).collect();
            let (stop, limits_json) = plan_sizes(&ctx, &cluster, &names).await;
            if let Some(code) = stop {
                return Ok(code);
            }
            fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
            fs::create_dir_all(&ctx.meta)
                .with_context(|| format!("create {}", ctx.meta.display()))?;
            fs::create_dir_all(&log_dir)?;
            let mut m = Manifest::load(&ctx.meta)?.unwrap_or_default();
            if m.node_compat != ctx.args.node_compat && !m.indices.is_empty() {
                bail!(
                    "{} already holds a dump {} --nodeCompat; use another directory",
                    root.display(),
                    if m.node_compat {
                        "made with"
                    } else {
                        "made without"
                    }
                );
            }
            m.node_compat = ctx.args.node_compat;
            if let Some(prev) = m.compression.filter(|c| *c != ctx.args.compression)
                && !m.indices.is_empty()
            {
                eprintln!(
                    "warning: this directory already holds a {prev:?} dump; new indices use {:?}",
                    ctx.args.compression
                );
            }
            m.tool = "searchdump-multi".into();
            m.version = env!("CARGO_PKG_VERSION").into();
            m.source = searchdump_rs::endpoint::redacted_endpoint(&ctx.args.input);
            m.layout = Some(ctx.args.layout);
            m.compression = Some(ctx.args.compression);
            m.server_limits = Some(limits_json);
            let mut todo = vec![];
            for t in selected {
                let done = m.indices.get(&t.name).is_some_and(|e| {
                    let d = root.join(&e.dir);
                    e.status == "done"
                        && e.types.iter().all(|k| match k.as_str() {
                            "data" if !e.data_parts.is_empty() => {
                                e.data_parts.iter().all(|p| d.join(p).is_file())
                            }
                            "data" => {
                                e.records == Some(0)
                                    || e.data_file.as_ref().is_some_and(|f| nonempty(&d.join(f)))
                            }
                            "analyzer" | "template" => d
                                .join(item_file_for(
                                    m.node_compat,
                                    &t.name,
                                    k,
                                    m.compression.unwrap_or(Compression::None),
                                ))
                                .is_file(),
                            k => meta_file(&d, &t.name, k).is_some(),
                        })
                });
                if ctx.args.resume && done {
                    println!("skip {} (already dumped)", t.name);
                    continue;
                }
                todo.push(t);
            }
            m.save(&ctx.meta)?;
            let manifest = Arc::new(Mutex::new(m));
            if ctx.meta != root {
                println!("manifest and cluster-wide files in {}", ctx.meta.display());
            }
            if !ctx.types.cluster.is_empty() {
                println!(
                    "dumping cluster-wide {} to {}",
                    ctx.types.cluster.join(", "),
                    ctx.meta.join(CLUSTER_DIR).display()
                );
                early = dump_cluster(&ctx, &cluster, &manifest).await;
            }
            println!(
                "dumping {} indices/data streams to {} ({} at a time, logs in {})",
                todo.len(),
                root.display(),
                ctx.args.parallel,
                log_dir.display()
            );
            // 1.2: a byte budget next to the --parallel bound: a worker holds its index's
            // estimated page bytes (KiB permits), so large-document indices run fewer at once
            // while small ones still run --parallel at once.
            let bytes = ctx.plan().budget.map(|b| {
                let total = (b.inflight_bytes >> 10).clamp(1, u32::MAX as u64 / 2) as u32;
                (Arc::new(tokio::sync::Semaphore::new(total as usize)), total)
            });
            for t in todo {
                let (ctx, cluster, root, manifest, sem, bytes) = (
                    ctx.clone(),
                    cluster.clone(),
                    root.clone(),
                    manifest.clone(),
                    sem.clone(),
                    bytes.clone(),
                );
                set.spawn(async move {
                    let _p = sem.acquire().await;
                    let _b = match &bytes {
                        Some((s, total)) => {
                            let need = ctx.plan().dump(ctx.stats_of(&t.name)).page_bytes >> 10;
                            s.clone()
                                .acquire_many_owned(need.clamp(1, *total as u64) as u32)
                                .await
                                .ok()
                        }
                        None => None,
                    };
                    let o = dump_one(&ctx, &cluster, &root, &manifest, &t).await;
                    (t.name, o)
                });
            }
        }
        Direction::Load | Direction::Copy => {
            let root = PathBuf::from(&ctx.args.input);
            if !root.is_dir() {
                bail!("dump directory does not exist: {}", root.display());
            }
            let mut found: Vec<Found> = if ctx.types.index.is_empty() {
                vec![]
            } else {
                discover(&root, &ctx.meta)?
                    .into_iter()
                    .filter(|f| ctx.sel.selects(&f.index))
                    .collect()
            };
            if ctx.args.order == Order::Desc {
                found.reverse();
            }
            let cluster_files: Vec<&str> = ctx
                .types
                .cluster
                .iter()
                .copied()
                .filter(|k| nonempty(&ctx.meta.join(CLUSTER_DIR).join(format!("{k}.json"))))
                .collect();
            if ctx.args.dry_run {
                if !cluster_files.is_empty() {
                    println!("cluster-wide, loaded first: {}", cluster_files.join(", "));
                }
                println!("{} indices/data streams to load:", found.len());
                for f in &found {
                    let parts = if f.data.len() > 1 {
                        format!(", {} data parts", f.data.len())
                    } else {
                        String::new()
                    };
                    let ds = if f.datastream { " [data stream]" } else { "" };
                    println!(
                        "  {}{ds} -> {}{}{} (from {}{parts})",
                        f.index,
                        ctx.args.prefix,
                        f.index,
                        ctx.args.suffix,
                        f.dir.display()
                    );
                }
                return Ok(0);
            }
            if found.is_empty() && cluster_files.is_empty() {
                eprintln!(
                    "nothing in {} matches --match '{}'",
                    root.display(),
                    ctx.args.match_re
                );
                return Ok(1);
            }
            let cluster = Arc::new(Cluster::new(
                &ctx.args.output,
                "SEARCHDUMP_OUTPUT",
                &ctx.args,
                &ctx.tls,
            )?);
            if ctx.types.index.contains(&"data") {
                let (stop, _) = plan_sizes(&ctx, &cluster, &[]).await;
                if let Some(code) = stop {
                    return Ok(code);
                }
            }
            fs::create_dir_all(&log_dir)?;
            if !ctx.types.cluster.is_empty() {
                early = load_cluster(&ctx, &cluster).await;
            }
            println!(
                "loading {} indices/data streams from {} ({} at a time, logs in {})",
                found.len(),
                root.display(),
                ctx.args.parallel,
                log_dir.display()
            );
            for f in found {
                let (ctx, cluster, sem) = (ctx.clone(), cluster.clone(), sem.clone());
                set.spawn(async move {
                    let _p = sem.acquire().await;
                    let o = load_one(&ctx, &cluster, &f).await;
                    (f.index, o)
                });
            }
        }
    }

    let (mut ok, mut skipped, mut failed) = (0, 0, vec![]);
    let mut report = |index: String, outcome: Outcome| {
        match outcome {
            Outcome::Done(s) => {
                ok += 1;
                println!("ok      {index}: {s}");
            }
            Outcome::Skipped(s) => {
                skipped += 1;
                println!("skipped {index}: {s}");
            }
            Outcome::Failed(s) => {
                eprintln!("FAILED  {index}: {s}");
                failed.push(index);
            }
        }
        let _ = std::io::stdout().flush();
    };
    for (index, outcome) in early {
        report(index, outcome);
    }
    while let Some(r) = set.join_next().await {
        let (index, outcome) = r?;
        report(index, outcome);
    }
    failed.sort();
    if !ctx
        .runner
        .runs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_empty()
    {
        match ctx.runner.save_runs() {
            Ok(p) => eprintln!("child runs: {}", p.display()),
            Err(e) => eprintln!("warning: could not write the child-run report: {e:#}"),
        }
    }
    println!(
        "\n{ok} done, {skipped} skipped, {} failed in {:.0?}{}",
        failed.len(),
        started.elapsed(),
        if failed.is_empty() {
            String::new()
        } else {
            format!(": {}", failed.join(", "))
        }
    );
    if cancelled.load(Ordering::SeqCst) {
        return Ok(130);
    }
    Ok(if failed.is_empty() { 0 } else { 1 })
}

fn chrono_stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn main() {
    let args = Args::parse();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let code = match rt.block_on(run(args)) {
        Ok(c) => c,
        Err(e) => {
            // L4-001: never show a registered secret (the key passphrase) in an error
            eprintln!(
                "searchdump-multi: {}",
                searchdump_rs::secrets::scrub(&format!("{e:#}"))
            );
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn interval_spaces_out_starts() {
        let p = Arc::new(Pacer::new(Duration::from_millis(500)));
        let t0 = tokio::time::Instant::now();
        let mut set = tokio::task::JoinSet::new();
        for _ in 0..3 {
            let p = p.clone();
            set.spawn(async move {
                p.wait().await;
                tokio::time::Instant::now()
            });
        }
        let mut at: Vec<Duration> = vec![];
        while let Some(t) = set.join_next().await {
            at.push(t.unwrap() - t0);
        }
        at.sort();
        assert_eq!(at[0], Duration::ZERO);
        assert!(at[1] >= Duration::from_millis(500), "{at:?}");
        assert!(at[2] >= Duration::from_millis(1000), "{at:?}");
        let none = Pacer::new(Duration::ZERO);
        none.wait().await;
        none.wait().await;
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_load_step_names_its_part_only_when_there_are_several() {
        let p = Path::new("/d/ix.data-2.json.gz");
        let one = format!(
            "{:#}",
            with_part(anyhow::anyhow!("interrupted: stopped"), 1, 1, p)
        );
        assert_eq!(one, "interrupted: stopped");
        let many = format!(
            "{:#}",
            with_part(anyhow::anyhow!("interrupted: stopped"), 3, 2, p)
        );
        assert_eq!(many, "part 2 (/d/ix.data-2.json.gz): interrupted: stopped");
    }
}
