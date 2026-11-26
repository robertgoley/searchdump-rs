//! `--dryRun` listings of `run_local` (split steps 12-13), printed to stdout.

use crate::*;

/// The dump dry run: cluster-wide files, then each selected target with the files it would write.
pub(crate) fn print_dump_plan(
    ctx: &Ctx,
    root: &Path,
    selected: &[Target],
    total: usize,
) -> Result<()> {
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
    for t in selected {
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
                    node_item_file(&format!("{}.split-N", t.name), k, ctx.args.compression)
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
    Ok(())
}

/// The load dry run: the cluster-wide files, then each index with its target name and parts.
pub(crate) fn print_load_plan(ctx: &Ctx, found: &[Found], cluster_files: &[&str]) {
    if !cluster_files.is_empty() {
        println!("cluster-wide, loaded first: {}", cluster_files.join(", "));
    }
    println!("{} indices/data streams to load:", found.len());
    for f in found {
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
}
