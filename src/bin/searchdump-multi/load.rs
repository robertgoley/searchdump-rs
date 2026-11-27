//! Loading one index (all its parts) or the cluster-wide types.

use crate::cli::Compression;
use crate::cli::Lifecycle;
use crate::cluster::Cluster;
use crate::ctx::Ctx;
use crate::ctx::Outcome;
use crate::discover::Found;
use crate::files::CLUSTER_DIR;
use crate::files::count_records;
use crate::files::file_name;
use crate::files::item_file;
use crate::files::mappings_body;
use crate::files::meta_file;
use crate::files::nonempty;
use crate::files::parse_data_file;
use crate::files::read_text;
use crate::flags::LIFECYCLE_DROP;
use crate::flags::child_data_flags;
use crate::flags::cluster_flags;
use crate::flags::strings;
use crate::manifest::Manifest;
use crate::selection::check_index_name;
use anyhow::{Context, Result, bail};
use std::path::Path;

/// Load the cluster-scoped files present under `ROOT/_cluster`, in CLUSTER_TYPES order.
pub(crate) async fn load_cluster(ctx: &Ctx, cluster: &Cluster) -> Vec<(String, Outcome)> {
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

pub(crate) async fn load_one(ctx: &Ctx, cluster: &Cluster, f: &Found) -> Outcome {
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
                    let mut extra = load_data_args(ctx, c);
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
pub(crate) fn with_part(e: anyhow::Error, parts: usize, n: usize, part: &Path) -> anyhow::Error {
    if parts > 1 {
        e.context(format!("part {n} ({})", part.display()))
    } else {
        e
    }
}

/// The data step's arguments for one part: its codec and the load plan's data flags merged with the
/// default/user flags.
pub(crate) fn load_data_args(ctx: &Ctx, c: Compression) -> Vec<String> {
    let mut extra = strings(c.flags());
    let size = ctx.plan().load();
    extra.extend(child_data_flags(ctx.plan(), &size, &ctx.runner));
    extra
}

#[cfg(test)]
mod tests {
    use super::*;

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
