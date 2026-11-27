//! One dump, load or copy over a local directory: the orchestration.

use crate::cli::Args;
use crate::cli::Direction;
use crate::ctx::Ctx;
use crate::ctx::Outcome;
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

mod dry_run;
mod dump_plan;
mod load_plan;
mod report;
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
            if let Some(code) = load_plan::start(&ctx, &log_dir, &sem, &mut set, &mut early).await?
            {
                return Ok(code);
            }
        }
    }

    let tally = report::collect(early, set).await?;
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
    println!("{}", tally.summary_line(started.elapsed()));
    Ok(tally.exit_code(interrupt.cancelled()))
}
