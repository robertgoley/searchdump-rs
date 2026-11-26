#!/usr/bin/env python3
"""Split step 13 (L4-S13): the load branch of run_local -> run_local/load_plan.rs (+ dry_run.rs
print_load_plan) and the result report -> run_local/report.rs (Tally, collect). usage: phase_b13.py SRC_DIR"""
import sys, re
sys.path.insert(0, __import__('os').path.dirname(__file__))
D = sys.argv[1]
p = f'{D}/run_local/mod.rs'
s = open(p).read()

def cut(src, a, b, start=0):
    i = src.index(a, start); j = src.index(b, i)
    return src[i:j], i, j

def dedent(block, n):
    out = []
    for l in block.split('\n'):
        if l.strip() == '': out.append('')
        else:
            assert l.startswith(' ' * n), repr(l); out.append(l[n:])
    return '\n'.join(out)

def refs(block):
    block = re.sub(r'&ctx(?![\w.])', 'ctx', block)
    block = re.sub(r'&log_dir(?![\w.])', 'log_dir', block)
    return block

head = '        Direction::Load | Direction::Copy => {\n'
ai = s.index(head)
tail_anchor = '    let (mut ok, mut skipped, mut failed) = (0, 0, vec![]);\n'
ti = s.index(tail_anchor)
arm_and_close = s[ai:ti]
# the arm ends with "        }\n    }\n\n" (arm brace, match brace)
m = re.search(r'\n        \}\n    \}\n\n$', arm_and_close)
assert m, repr(arm_and_close[-40:])
inner = arm_and_close[len(head):m.start() + 1]
inner = dedent(inner, 8)

disc, _, _ = cut(inner, '    let mut found: Vec<Found> = if ctx.types.index.is_empty() {\n', '    let cluster_files: Vec<&str> = ctx\n')
cf, _, _ = cut(inner, '    let cluster_files: Vec<&str> = ctx\n', '    if ctx.args.dry_run {\n')
dry, _, _ = cut(inner, '    if ctx.args.dry_run {\n', '    if found.is_empty() && cluster_files.is_empty() {\n')
dry_body = dry[len('    if ctx.args.dry_run {\n'):].rstrip()
assert dry_body.endswith('return Ok(0);\n    }')
dry_body = dedent(dry_body[:-len('return Ok(0);\n    }')].rstrip() + '\n', 4)

start = inner
start = start.replace(disc, '    let found = discover_selected(ctx, &root)?;\n')
start = start.replace(cf, '    let cluster_files = cluster_files_present(ctx);\n')
start = start.replace(dry, '    if ctx.args.dry_run {\n        dry_run::print_load_plan(ctx, &found, &cluster_files);\n        return Ok(Some(0));\n    }\n')
for a, b in [('return Ok(1);', 'return Ok(Some(1));'), ('return Ok(code);', 'return Ok(Some(code));'),
             ('    early = load_cluster(', '    *early = load_cluster(')]:
    assert a in start, a
    start = start.replace(a, b)
k = start.index('    for f in found {\n')
start = refs(start[:k]) + start[k:]

disc_fn = disc.rstrip('\n') + '\n    Ok(found)\n'
cf_body = cf.rstrip('\n')
assert cf_body.startswith('    let cluster_files: Vec<&str> = ') and cf_body.endswith(';')
cf_expr = '    ' + cf_body[len('    let cluster_files: Vec<&str> = '):-1].lstrip()

load_plan = f'''//! The load (and copy) side of `run_local` (split step 13): find the selected indices and parts, the dry
//! run, size plans, the cluster-wide load and one task per index.

use crate::*;

use super::dry_run;

/// Runs the load set-up and spawns one task per index into `set`; `Some(code)` ends the run early
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

/// The indices in the dump directory that `--match` selects (in `--order`); none without index types.
pub(crate) fn discover_selected(ctx: &Ctx, root: &Path) -> Result<Vec<Found>> {{
{disc_fn}}}

/// The selected cluster-wide types that have a non-empty file under `META/_cluster/`.
pub(crate) fn cluster_files_present(ctx: &Ctx) -> Vec<&'static str> {{
{cf_expr}
}}
'''
open(f'{D}/run_local/load_plan.rs', 'w').write(load_plan)

dp = f'{D}/run_local/dry_run.rs'
d = open(dp).read().rstrip('\n') + f'''

/// The load dry run: the cluster-wide files, then each index with its target name and parts.
pub(crate) fn print_load_plan(ctx: &Ctx, found: &[Found], cluster_files: &[&str]) {{
{dry_body.rstrip()}
}}
'''
open(dp, 'w').write(d)

# report: the tail of run_local after the match
tail = s[ti:]
exp_tail_start = tail_anchor
save_runs, _, _ = cut(tail, '    if !ctx\n        .runner\n        .runs\n', '    println!(\n        "\\n{ok} done')
new_tail = ('    let tally = report::collect(early, set).await?;\n' + save_runs +
            '    println!("{}", tally.summary_line(started.elapsed()));\n'
            '    Ok(tally.exit_code(interrupt.cancelled()))\n}\n')
# check the old tail is what report.rs reproduces
assert '    failed.sort();\n' in tail and 'if interrupt.cancelled() {\n        return Ok(130);' in tail
fn_end = tail.index('\n}\n') + 3
s = s[:ai] + '''        Direction::Load | Direction::Copy => {
            if let Some(code) =
                load_plan::start(&ctx, &log_dir, &sem, &mut set, &mut early).await?
            {
                return Ok(code);
            }
        }
    }

''' + new_tail + tail[fn_end:]
s = s.replace('mod dump_plan;\n', 'mod dump_plan;\nmod load_plan;\nmod report;\n', 1)
open(p, 'w').write(s)

report = r'''//! The result report of `run_local` (split step 13): ok/skipped lines on stdout, FAILED on stderr (each
//! flushed as it arrives, cluster-wide results first), the summary line and the exit code.

use crate::*;

#[derive(Debug, Default)]
pub(crate) struct Tally {
    pub(crate) ok: usize,
    pub(crate) skipped: usize,
    pub(crate) failed: Vec<String>,
}

impl Tally {
    /// Prints one result and counts it.
    pub(crate) fn record(&mut self, index: String, outcome: Outcome) {
        match outcome {
            Outcome::Done(s) => {
                self.ok += 1;
                println!("ok      {index}: {s}");
            }
            Outcome::Skipped(s) => {
                self.skipped += 1;
                println!("skipped {index}: {s}");
            }
            Outcome::Failed(s) => {
                eprintln!("FAILED  {index}: {s}");
                self.failed.push(index);
            }
        }
        let _ = std::io::stdout().flush();
    }

    /// The last line of a run (printed after an empty line), failures sorted by name.
    pub(crate) fn summary_line(&self, elapsed: Duration) -> String {
        format!(
            "\n{} done, {} skipped, {} failed in {:.0?}{}",
            self.ok,
            self.skipped,
            self.failed.len(),
            elapsed,
            if self.failed.is_empty() {
                String::new()
            } else {
                format!(": {}", self.failed.join(", "))
            }
        )
    }

    /// 130 after Ctrl-C (even with failures), else 1 with failures, else 0.
    pub(crate) fn exit_code(&self, cancelled: bool) -> i32 {
        if cancelled {
            return 130;
        }
        if self.failed.is_empty() { 0 } else { 1 }
    }
}

/// Reports the cluster-wide results, then each task as it finishes; a panicked task (JoinError)
/// ends the run with an error (exit 2), as before.
pub(crate) async fn collect(
    early: Vec<(String, Outcome)>,
    mut set: tokio::task::JoinSet<(String, Outcome)>,
) -> Result<Tally> {
    let mut tally = Tally::default();
    for (index, outcome) in early {
        tally.record(index, outcome);
    }
    while let Some(r) = set.join_next().await {
        let (index, outcome) = r?;
        tally.record(index, outcome);
    }
    tally.failed.sort();
    Ok(tally)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_and_summary_line() {
        let mut t = Tally::default();
        assert_eq!(t.exit_code(false), 0);
        assert_eq!(t.exit_code(true), 130);
        t.ok = 2;
        t.skipped = 1;
        assert_eq!(
            t.summary_line(Duration::from_millis(1400)),
            "\n2 done, 1 skipped, 0 failed in 1s"
        );
        t.failed = vec!["a".into(), "b".into()];
        assert_eq!(t.exit_code(false), 1);
        assert_eq!(t.exit_code(true), 130, "130 wins over failures");
        assert_eq!(
            t.summary_line(Duration::from_secs(3)),
            "\n2 done, 1 skipped, 2 failed in 3s: a, b"
        );
    }

    #[tokio::test]
    async fn collect_reports_early_results_first_and_sorts_failures() {
        let mut set = tokio::task::JoinSet::new();
        set.spawn(async { ("z".to_string(), Outcome::Failed("x".into())) });
        let early = vec![
            ("m".to_string(), Outcome::Failed("y".into())),
            ("_cluster/policy".to_string(), Outcome::Done("1".into())),
        ];
        let t = collect(early, set).await.unwrap();
        assert_eq!((t.ok, t.skipped), (1, 0));
        assert_eq!(t.failed, vec!["m".to_string(), "z".to_string()]);
    }
}
'''
open(f'{D}/run_local/report.rs', 'w').write(report)
print('ok')
