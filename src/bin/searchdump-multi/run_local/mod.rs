//! One dump, load or copy over a local directory: the orchestration.

use crate::*;

mod dry_run;
mod dump_plan;
mod setup;

pub(crate) async fn run_local(args: Args) -> Result<i32> {
    setup::validate_local(&args)?;
    let types = setup::run_types(&args)?;
    let sel = setup::selector(&args)?;
    setup::warn_url_credentials(&args);
    let log_dir = setup::log_dir(&args);
    let (side, tls) = setup::cluster_tls(&args)?;
    let interrupt = setup::Interrupt::install();
    let runner = setup::build_runner(&args, &tls, side, &log_dir, &interrupt)?;
    let ctx = Arc::new(Ctx::new(args, tls, types, sel, runner));
    let started = Instant::now();
    let sem = Arc::new(tokio::sync::Semaphore::new(ctx.args.parallel));
    let mut set = tokio::task::JoinSet::new();
    // Cluster-scoped results, reported with the per-index ones.
    let mut early: Vec<(String, Outcome)> = vec![];

    match ctx.args.direction {
        Direction::Dump => {
            if let Some(code) = dump_plan::start(&ctx, &log_dir, &sem, &mut set, &mut early).await?
            {
                return Ok(code);
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
    if interrupt.cancelled() {
        return Ok(130);
    }
    Ok(if failed.is_empty() { 0 } else { 1 })
}
