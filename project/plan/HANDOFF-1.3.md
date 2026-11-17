# Handoff: 1.3 work on grokbox (local branch `release/1.3`)

Started 2026-11-17 16:15 ET (Robert t220u "Start all those on your VM"). Branch `release/1.3` is LOCAL
ONLY, cut from `origin/main` 53bb69a, worktree `/workspace/rel-1.3`. **Nothing is pushed** (no remote
branch, no main change, no tags, no PRs, no reruns): Robert decides on pushes. Commits `L3-NNN:`.
Scratch (not committed): `/workspace/rel-1.3-work` (release binaries `bin/1.1.0`, `bin/1.2.0`, A/B
outputs, suite logs). Env: `source /workspace/perf-1.2/env.sh` (isolated podman store, port 19252).

## Items and state
| item | prio | state | evidence |
|---|---|---|---|
| 1.3-G OS3 plain 1.2 slowdown | P1 | not reproducible: A/B 1.00-1.02x, identical traces, full fresh 1.2.0 pass normal (L3-002); 1.1.0 pass running via chain1.sh | results/local/L3-001.md, L3-002.md |
| 1.3-G stalls (~17 s p8 / ~44 s p1, OS 2/3, all releases) | P1 | not started | |
| 1.3-D+/C/E small-heap auto collapse | P1 | analysis only: os3 load 429s are split warnings (cb=0, not breaker; lowheap counts any "429" text, retries counts only "retrying in" -> retries=0) | relcmp lowheap/1.2.0.json |
| 1.3-R1 ES 8.6 p8 restore | P2 | harness DELETE wait + deletes.log done (L3-002); 20-rep reruns not started | |
| Reporting | P3 | done: scripts/perf/relcmp_report.py -> relcmp-20261117/REPORT-EXTRA.md | L3-002 |

## Next
Background chain `/workspace/rel-1.3-work/chain1.sh` (log `chain1.log`): suite 1.1.0 os3-plain, then lowheap
os3 (1.2 auto / 1.1 none / 1.2 min / 1.2 off) and os2,es912 1.2 auto, run ids `l3-*`, `--keep`
(work dirs under /var/tmp/searchdump-load/l3-*).
1. Collect the chain results.
2. lowheap.py os3 256m/384m with `LOAD_BIN_DIR=/workspace/rel-1.3-work/bin/1.2.0` (--mode auto, min, off)
   and 1.1.0 (--mode none), `--keep` to read the child logs (429 bodies: indexing pressure vs queue).
