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
| 1.3-D+/C/E small-heap auto collapse | P1 | reproduced on grokbox (l3-os3-12-auto: 5 bulk splits per os3 load at 256m and 384m, 6.4 MB -> 170 KB cap). retries=0 explained: 1.2 splits log a WARN without "retrying in" and without the error body (so cb=0 too). L3-003 opt-in experiments (SEARCHDUMP_LIMITS_EXPERIMENT=floor/configured/growback), L3-004 cause in split/halve warnings. chain2.sh measures them | results/load/l3-* (not committed yet) |
| 1.3-R1 ES 8.6 p8 restore | P2 | harness DELETE wait + deletes.log done (L3-002); 20-rep reruns not started | |
| Reporting | P3 | done: scripts/perf/relcmp_report.py -> relcmp-20261117/REPORT-EXTRA.md | L3-002 |

## Next
Background chains `/workspace/rel-1.3-work/chain1.sh` then `chain2.sh` (experiments on os3 with bin/l3exp = L3-004 build; 1.3-R1 ES 8.6 p8 x20 old harness 1.1.0/1.2.0 from worktree rel-1.3-work/oldharness at 53bb69a, then new harness 1.2.0). Chain1 (log `chain1.log`): suite 1.1.0 os3-plain, then lowheap
os3 (1.2 auto / 1.1 none / 1.2 min / 1.2 off) and os2,es912 1.2 auto, run ids `l3-*`, `--keep`
(work dirs under /var/tmp/searchdump-load/l3-*).
1. Collect the chain results.
2. lowheap.py os3 256m/384m with `LOAD_BIN_DIR=/workspace/rel-1.3-work/bin/1.2.0` (--mode auto, min, off)
   and 1.1.0 (--mode none), `--keep` to read the child logs (429 bodies: indexing pressure vs queue).

## Update 17:50 ET
- chain1/2/3 finished. Commits since: L3-007 (1.3-G same-host 1.1 vs 1.2 = 1.00), L3-008 (multi parent 429 retry
  bug fix), L3-009 (1.3-R1 x20: 0 failures).
- 1.3-D+: lowheap runs under results/load/l3-* (compare with scripts/load/lowheap_cmp.py); repeated interleaved
  runs via scripts/load/lowheap_reps.py with bin/l3exp3 (= c205454 + experiments) are next; then stall loop
  (scripts/perf/stall_loop.py) for the OS 2/3 restore stalls.
