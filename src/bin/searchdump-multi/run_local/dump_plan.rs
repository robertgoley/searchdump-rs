//! The dump side of `run_local` (split step 12): list and select the targets, the dry run, size plans,
//! the manifest, `--resume`, the cluster-wide dump and one task per target.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::dry_run;
use crate::cli::Compression;
use crate::cli::MatchType;
use crate::cli::Order;
use crate::cluster::Cluster;
use crate::ctx::Ctx;
use crate::ctx::Outcome;
use crate::dump::dump_cluster;
use crate::dump::dump_one;
use crate::files::CLUSTER_DIR;
use crate::files::item_file_for;
use crate::files::meta_file;
use crate::files::nonempty;
use crate::manifest::Manifest;
use crate::planner::plan_sizes;
use crate::selection::Target;
use crate::selection::TargetKind;
use crate::selection::check_index_name;
use crate::selection::select_targets;

/// Runs the dump set-up and spawns one task per target into `set`; `Some(code)` ends the run early
/// (dry run 0, nothing matched 1, `--serverLimits=check` 0/2).
pub(crate) async fn start(
    ctx: &Arc<Ctx>,
    log_dir: &Path,
    sem: &Arc<tokio::sync::Semaphore>,
    set: &mut tokio::task::JoinSet<(String, Outcome)>,
    early: &mut Vec<(String, Outcome)>,
) -> Result<Option<i32>> {
    let cluster = Arc::new(Cluster::new(
        &ctx.args.input,
        "SEARCHDUMP_INPUT",
        &ctx.args,
        &ctx.tls,
    )?);
    let (selected, total) = select_dump_targets(ctx, &cluster).await?;
    let root = PathBuf::from(&ctx.args.output);
    if ctx.args.dry_run {
        dry_run::print_dump_plan(ctx, &root, &selected, total)?;
        return Ok(Some(0));
    }
    if selected.is_empty() && ctx.types.cluster.is_empty() {
        eprintln!(
            "nothing matches --match '{}' (of {total} indices and data streams)",
            ctx.args.match_re,
        );
        return Ok(Some(1));
    }
    let names: Vec<String> = selected.iter().map(|t| t.name.clone()).collect();
    let (stop, limits_json) = plan_sizes(ctx, &cluster, &names).await;
    if let Some(code) = stop {
        return Ok(Some(code));
    }
    fs::create_dir_all(&root).with_context(|| format!("create {}", root.display()))?;
    fs::create_dir_all(&ctx.meta).with_context(|| format!("create {}", ctx.meta.display()))?;
    fs::create_dir_all(log_dir)?;
    let m = prepare_manifest(ctx, &root, limits_json)?;
    let mut todo = vec![];
    for t in selected {
        let done = already_done(&m, &root, &t.name);
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
        *early = dump_cluster(ctx, &cluster, &manifest).await;
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
    let bytes = byte_budget(ctx);
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
    Ok(None)
}

/// The cluster's indices, aliases and data streams selected for this dump (in `--order`), and how
/// many indices and data streams the cluster has.
pub(crate) async fn select_dump_targets(
    ctx: &Ctx,
    cluster: &Cluster,
) -> Result<(Vec<Target>, usize)> {
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
    Ok((selected, total))
}

/// The manifest of the dump directory, checked against this run and updated with its header fields.
pub(crate) fn prepare_manifest(
    ctx: &Ctx,
    root: &Path,
    limits_json: serde_json::Value,
) -> Result<Manifest> {
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
    Ok(m)
}

/// `--resume`: whether `name` is already completely dumped under `root` (manifest status, data
/// parts or file, and every meta file present).
pub(crate) fn already_done(m: &Manifest, root: &Path, name: &str) -> bool {
    m.indices.get(name).is_some_and(|e| {
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
                        name,
                        k,
                        m.compression.unwrap_or(Compression::None),
                    ))
                    .is_file(),
                k => meta_file(&d, name, k).is_some(),
            })
    })
}

/// 1.2: the byte budget next to the `--parallel` bound, in KiB permits (and their total).
pub(crate) fn byte_budget(ctx: &Ctx) -> Option<(Arc<tokio::sync::Semaphore>, u32)> {
    ctx.plan().budget.map(|b| {
        let total = (b.inflight_bytes >> 10).clamp(1, u32::MAX as u64 / 2) as u32;
        (Arc::new(tokio::sync::Semaphore::new(total as usize)), total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::IndexEntry;

    fn entry(types: &[&str]) -> IndexEntry {
        IndexEntry {
            dir: "sub".into(),
            types: types.iter().map(|s| s.to_string()).collect(),
            status: "done".into(),
            ..Default::default()
        }
    }

    #[test]
    fn already_done_needs_every_file_of_a_finished_index() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        fs::create_dir_all(root.join("sub")).unwrap();
        let mut m = Manifest::default();
        assert!(!already_done(&m, root, "ix"), "not in the manifest");

        // single data file: present and non-empty, or a 0-record dump
        let mut e = entry(&["data"]);
        e.data_file = Some("ix.data.json.gz".into());
        m.indices.insert("ix".into(), e.clone());
        assert!(!already_done(&m, root, "ix"), "data file missing");
        fs::write(root.join("sub/ix.data.json.gz"), "x").unwrap();
        assert!(already_done(&m, root, "ix"));
        e.status = "failed".into();
        m.indices.insert("ix".into(), e.clone());
        assert!(!already_done(&m, root, "ix"), "status must be done");
        let mut empty = entry(&["data"]);
        empty.records = Some(0);
        m.indices.insert("e".into(), empty);
        assert!(already_done(&m, root, "e"), "0 records needs no file");

        // split parts: all present
        let mut s = entry(&["data"]);
        s.data_parts = vec![
            "s.data.split-0.json.gz".into(),
            "s.data.split-1.json.gz".into(),
        ];
        m.indices.insert("s".into(), s);
        fs::write(root.join("sub/s.data.split-0.json.gz"), "x").unwrap();
        assert!(!already_done(&m, root, "s"), "part 1 missing");
        fs::write(root.join("sub/s.data.split-1.json.gz"), "x").unwrap();
        assert!(already_done(&m, root, "s"));

        // meta files and the analyzer file (named by the manifest's codec and layout)
        m.indices
            .insert("t".into(), entry(&["settings", "analyzer"]));
        fs::write(root.join("sub/t.settings.json"), "{}").unwrap();
        assert!(!already_done(&m, root, "t"), "analyzer file missing");
        fs::write(
            root.join("sub")
                .join(item_file_for(false, "t", "analyzer", Compression::None)),
            "{}",
        )
        .unwrap();
        assert!(already_done(&m, root, "t"));
    }
}
