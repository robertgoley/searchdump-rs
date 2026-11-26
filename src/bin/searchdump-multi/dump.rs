//! Dumping one index or the cluster-wide types.

use crate::*;

/// 1.1-7: `SEARCHDUMP_MULTI_DEBUG=1` prints where each record count came from.
pub(crate) fn debug_counts() -> bool {
    std::env::var_os("SEARCHDUMP_MULTI_DEBUG").is_some_and(|v| v != "0" && !v.is_empty())
}

pub(crate) async fn dump_one(
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
pub(crate) async fn dump_cluster(
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
