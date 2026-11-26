//! One dump, load or copy over a local directory: the orchestration.

use crate::*;

pub(crate) async fn run_local(args: Args) -> Result<i32> {
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
