//! Per-index size plans (`--serverLimits`) and their manifest records.

use crate::*;

/// 1.2: probe the cluster once, read every target's stats in one call, and set the plan
/// (ROADMAP-1.2 section 4). `targets` are the indices to dump (empty for a load). Returns the
/// exit code to stop with for `--serverLimits=check` (0, or 2 when a flag after `--` exceeds
/// the limits) and the manifest's `server_limits` object.
pub(crate) async fn plan_sizes(
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

/// 1.2: the per-index note when an index gets a page below the 1.1 default (large documents
/// or the budget), and a warning when the user's `-- --limit` is above the safe page.
pub(crate) fn note_size(
    ctx: &Ctx,
    index: &str,
    st: Option<sizing::Stats>,
    size: &sizing::IndexSize,
) {
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

pub(crate) fn limits_max_page() -> u64 {
    searchdump_rs::limits::MULTI_MAX_PAGE
}

/// The manifest's per-index `sizing` object.
pub(crate) fn sizing_json(size: &sizing::IndexSize, user: &[String]) -> serde_json::Value {
    let user_limit = user.iter().any(|a| flag_name(a) == Some("--limit"));
    serde_json::json!({
        "limit": size.limit,
        "max_bulk_bytes": size.max_bulk_bytes,
        "reason": if user_limit { "user" } else { size.reason },
    })
}
