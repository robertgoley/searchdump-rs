#!/usr/bin/env python3
"""Split step 12 (L4-S12): the dump branch of run_local -> run_local/dump_plan.rs (+ dry_run.rs
print_dump_plan), cut by text anchors; code verbatim apart from de-indenting, early exits returning
`Some(code)`, and `&ctx`/`&log_dir` (now references) losing the extra borrow.
usage: phase_b12.py SRC_DIR"""
import sys, re
D = sys.argv[1]
p = f'{D}/run_local/mod.rs'
s = open(p).read()

TESTS = r'''
#[cfg(test)]
mod tests {
    use super::*;

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
        s.data_parts = vec!["s.data.split-0.json.gz".into(), "s.data.split-1.json.gz".into()];
        m.indices.insert("s".into(), s);
        fs::write(root.join("sub/s.data.split-0.json.gz"), "x").unwrap();
        assert!(!already_done(&m, root, "s"), "part 1 missing");
        fs::write(root.join("sub/s.data.split-1.json.gz"), "x").unwrap();
        assert!(already_done(&m, root, "s"));

        // meta files and the analyzer file (named by the manifest's codec and layout)
        m.indices.insert("t".into(), entry(&["settings", "analyzer"]));
        fs::write(root.join("sub/t.settings.json"), "{}").unwrap();
        assert!(!already_done(&m, root, "t"), "analyzer file missing");
        fs::write(
            root.join("sub").join(item_file_for(false, "t", "analyzer", Compression::None)),
            "{}",
        )
        .unwrap();
        assert!(already_done(&m, root, "t"));
    }
}
'''

def cut(src, a, b, start=0):
    i = src.index(a, start); j = src.index(b, i)
    return src[i:j], i, j

def dedent(block, n):
    out = []
    for l in block.split('\n'):
        if l.strip() == '':
            out.append('')
        else:
            assert l.startswith(' ' * n), repr(l)
            out.append(l[n:])
    return '\n'.join(out)

def indent(block, n):
    return '\n'.join((' ' * n + l) if l.strip() else '' for l in block.split('\n'))

def refs(block):
    block = re.sub(r'&ctx(?![\w.])', 'ctx', block)
    block = re.sub(r'&log_dir(?![\w.])', 'log_dir', block)
    return block

arm, ai, aj = cut(s, '        Direction::Dump => {\n', '        Direction::Load | Direction::Copy => {\n')
inner = arm[len('        Direction::Dump => {\n'):]
assert inner.rstrip().endswith('}')
inner = inner.rstrip()[:-1].rstrip() + '\n'   # drop the arm's closing brace
inner = dedent(inner, 8)                       # now 4-space body

sel, si, sj = cut(inner, '    let mt = &ctx.args.match_type;\n', '    let root = PathBuf::from(&ctx.args.output);\n')
dry, di, dj = cut(inner, '    if ctx.args.dry_run {\n', '    if selected.is_empty() && ctx.types.cluster.is_empty() {\n')
assert dry.rstrip().endswith('return Ok(0);\n    }'.rstrip()) or dry.rstrip().endswith('}')
dry_body = dry[len('    if ctx.args.dry_run {\n'):].rstrip()
assert dry_body.endswith('return Ok(0);\n    }')
dry_body = dry_body[:-len('return Ok(0);\n    }')].rstrip() + '\n'
dry_body = dedent(dry_body, 4)
assert '    for t in &selected {' in dry_body
dry_body = dry_body.replace('    for t in &selected {', '    for t in selected {')  # selected: &[Target] now
man, mi, mj = cut(inner, '    let mut m = Manifest::load(&ctx.meta)?.unwrap_or_default();\n', '    let mut todo = vec![];\n')
done, oi, oj = cut(inner, '        let done = m.indices.get(&t.name).is_some_and(|e| {\n', '        if ctx.args.resume && done {\n')
bud, bi, bj = cut(inner, '    let bytes = ctx.plan().budget.map(|b| {\n', '    for t in todo {\n')

start = inner
start = start.replace(sel, '    let (selected, total) = select_dump_targets(ctx, &cluster).await?;\n')
start = start.replace(dry, '    if ctx.args.dry_run {\n        dry_run::print_dump_plan(ctx, &root, &selected, total)?;\n        return Ok(Some(0));\n    }\n')
start = start.replace(man, '    let m = prepare_manifest(ctx, &root, limits_json)?;\n')
start = start.replace(done, '        let done = already_done(&m, &root, &t.name);\n')
start = start.replace(bud, '    let bytes = byte_budget(ctx);\n')
for a, b in [('return Ok(1);', 'return Ok(Some(1));'), ('return Ok(code);', 'return Ok(Some(code));'),
             ('    early = dump_cluster(', '    *early = dump_cluster(')]:
    assert a in start, a
    start = start.replace(a, b)
k = start.index('    for t in todo {\n')
start = refs(start[:k]) + start[k:]   # the spawned tasks own an Arc<Ctx> clone: keep `&ctx` there

# select_dump_targets: returns (selected, total)
sel_fn = refs(sel).rstrip('\n') + '\n    Ok((selected, total))\n'
man_body = man.rstrip('\n')
man_fn = refs(man_body) + '\n    Ok(m)\n'
done_expr = dedent(done.rstrip('\n'), 4)
assert done_expr.startswith('    let done = ') and done_expr.endswith(';')
done_expr = done_expr[len('    let done = '):-1]
done_expr = done_expr.replace('m.indices.get(&t.name)', 'm.indices.get(name)').replace('&t.name,', 'name,')
assert 't.name' not in done_expr
bud_body = bud.rstrip('\n')
assert bud_body.startswith('    let bytes = ') and bud_body.endswith(';')
bud_expr = bud_body[len('    let bytes = '):-1]
# the 1.2 comment before `let bytes` stays in start (it describes the semaphore use)

dump_plan = f'''//! The dump side of `run_local` (split step 12): list and select the targets, the dry run, size plans,
//! the manifest, `--resume`, the cluster-wide dump and one task per target.

use crate::*;

use super::dry_run;

/// Runs the dump set-up and spawns one task per target into `set`; `Some(code)` ends the run early
/// (dry run 0, nothing matched 1, `--serverLimits=check` 0/2).
pub(crate) async fn start(
    ctx: &Arc<Ctx>,
    log_dir: &Path,
    sem: &Arc<tokio::sync::Semaphore>,
    set: &mut tokio::task::JoinSet<(String, Outcome)>,
    early: &mut Vec<(String, Outcome)>,
) -> Result<Option<i32>> {{
{start.rstrip()}
    Ok(None)
}}

/// The cluster's indices, aliases and data streams selected for this dump (in `--order`), and how
/// many indices and data streams the cluster has.
pub(crate) async fn select_dump_targets(ctx: &Ctx, cluster: &Cluster) -> Result<(Vec<Target>, usize)> {{
{sel_fn}}}

/// The manifest of the dump directory, checked against this run and updated with its header fields.
pub(crate) fn prepare_manifest(ctx: &Ctx, root: &Path, limits_json: serde_json::Value) -> Result<Manifest> {{
{man_fn}}}

/// `--resume`: whether `name` is already completely dumped under `root` (manifest status, data
/// parts or file, and every meta file present).
pub(crate) fn already_done(m: &Manifest, root: &Path, name: &str) -> bool {{
{done_expr}
}}

/// 1.2: the byte budget next to the `--parallel` bound, in KiB permits (and their total).
pub(crate) fn byte_budget(ctx: &Ctx) -> Option<(Arc<tokio::sync::Semaphore>, u32)> {{
    {bud_expr.strip()}
}}
'''
dump_plan += TESTS
open(f'{D}/run_local/dump_plan.rs', 'w').write(dump_plan)

dry_run = f'''//! `--dryRun` listings of `run_local` (split steps 12-13), printed to stdout.

use crate::*;

/// The dump dry run: cluster-wide files, then each selected target with the files it would write.
pub(crate) fn print_dump_plan(ctx: &Ctx, root: &Path, selected: &[Target], total: usize) -> Result<()> {{
{refs(dry_body).rstrip()}
    Ok(())
}}
'''
open(f'{D}/run_local/dry_run.rs', 'w').write(dry_run)

new_arm = '''        Direction::Dump => {
            if let Some(code) =
                dump_plan::start(&ctx, &log_dir, &sem, &mut set, &mut early).await?
            {
                return Ok(code);
            }
        }
'''
s = s[:ai] + new_arm + s[aj:]
s = s.replace('mod setup;\n', 'mod dry_run;\nmod dump_plan;\nmod setup;\n', 1)
open(p, 'w').write(s)
print('ok')
