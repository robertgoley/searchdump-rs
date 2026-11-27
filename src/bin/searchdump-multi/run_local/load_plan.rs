//! The load (and copy) side of `run_local` (split step 13): find the selected indices and parts, the dry
//! run, size plans, the cluster-wide load and one task per index.

use anyhow::{Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::dry_run;
use crate::cli::Order;
use crate::cluster::Cluster;
use crate::ctx::Ctx;
use crate::ctx::Outcome;
use crate::discover::Found;
use crate::discover::discover;
use crate::files::CLUSTER_DIR;
use crate::files::nonempty;
use crate::load::load_cluster;
use crate::load::load_one;
use crate::planner::plan_sizes;

/// Runs the load set-up and spawns one task per index into `set`; `Some(code)` ends the run early
/// (dry run 0, nothing matched 1, `--serverLimits=check` 0/2).
pub(crate) async fn start(
    ctx: &Arc<Ctx>,
    log_dir: &Path,
    sem: &Arc<tokio::sync::Semaphore>,
    set: &mut tokio::task::JoinSet<(String, Outcome)>,
    early: &mut Vec<(String, Outcome)>,
) -> Result<Option<i32>> {
    let root = PathBuf::from(&ctx.args.input);
    if !root.is_dir() {
        bail!("dump directory does not exist: {}", root.display());
    }
    let found = discover_selected(ctx, &root)?;
    let cluster_files = cluster_files_present(ctx);
    if ctx.args.dry_run {
        dry_run::print_load_plan(ctx, &found, &cluster_files);
        return Ok(Some(0));
    }
    if found.is_empty() && cluster_files.is_empty() {
        eprintln!(
            "nothing in {} matches --match '{}'",
            root.display(),
            ctx.args.match_re
        );
        return Ok(Some(1));
    }
    let cluster = Arc::new(Cluster::new(
        &ctx.args.output,
        "SEARCHDUMP_OUTPUT",
        &ctx.args,
        &ctx.tls,
    )?);
    if ctx.types.index.contains(&"data") {
        let (stop, _) = plan_sizes(ctx, &cluster, &[]).await;
        if let Some(code) = stop {
            return Ok(Some(code));
        }
    }
    fs::create_dir_all(log_dir)?;
    if !ctx.types.cluster.is_empty() {
        *early = load_cluster(ctx, &cluster).await;
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
    Ok(None)
}

/// The indices in the dump directory that `--match` selects (in `--order`); none without index types.
pub(crate) fn discover_selected(ctx: &Ctx, root: &Path) -> Result<Vec<Found>> {
    let mut found: Vec<Found> = if ctx.types.index.is_empty() {
        vec![]
    } else {
        discover(root, &ctx.meta)?
            .into_iter()
            .filter(|f| ctx.sel.selects(&f.index))
            .collect()
    };
    if ctx.args.order == Order::Desc {
        found.reverse();
    }
    Ok(found)
}

/// The selected cluster-wide types that have a non-empty file under `META/_cluster/`.
pub(crate) fn cluster_files_present(ctx: &Ctx) -> Vec<&'static str> {
    ctx.types
        .cluster
        .iter()
        .copied()
        .filter(|k| nonempty(&ctx.meta.join(CLUSTER_DIR).join(format!("{k}.json"))))
        .collect()
}
