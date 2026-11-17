# Handoff: 1.3 work on grokbox (local branch `release/1.3`)

Started 2026-11-17 16:15 ET (Robert t220u "Start all those on your VM"). Branch `release/1.3` is LOCAL
ONLY, cut from `origin/main` 53bb69a, worktree `/workspace/rel-1.3`. **Nothing is pushed** (no remote
branch, no main change, no tags, no PRs, no reruns): Robert decides on pushes. Commits `L3-NNN:`.
Scratch (not committed): `/workspace/rel-1.3-work` (release binaries `bin/1.1.0`, `bin/1.2.0`, A/B
outputs, suite logs). Env: `source /workspace/perf-1.2/env.sh` (isolated podman store, port 19252).

## Items and state (updated 18:25 ET; full write-up in results/local/L3-SUMMARY.md)
| item | prio | state | evidence |
|---|---|---|---|
| 1.3-G OS3 plain 1.2 slowdown | P1 | done: not a 1.2 regression (A/B 0.91-1.08, identical traces; same-host Suite A 1.2/1.1 median 1.00) | L3-001, L3-002, L3-007 |
| 1.3-G stalls (OS 2/3, all releases) | P1 | traced on grokbox, not reproduced (60 restores, worst 1.6x median); kvmsvr1 rerun of stall_loop.py proposed | L3-010, L3-012 |
| 1.3-D+/C/E small-heap collapse | P1 | done: root cause = split on real-memory parent-breaker 429 that never grows back; options measured, retryfirst+growback best (median 6.7 s vs 16.7 s 1.2.0, 4.7 s 1.1.0); all opt-in | L3-003, L3-004, L3-006, L3-011 |
| multi parent 429 on _refresh | - | fixed (L3-008), proposed for 1.3 | L3-008 |
| 1.3-R1 ES 8.6 p8 restore | P2 | done: 0/63 failures, DELETE wait in harness; propose close | L3-002, L3-009 |
| Reporting | P3 | done | L3-002 |

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

## Update 18:25 ET
- Reps finished (L3-011), stall tracing done (L3-012), clippy -D warnings and fmt --check clean. Containers removed.
- Remaining (not started): settle/re-probe option, 384m reps, R1 pause/memory-pressure variants, per-child uuid
  logging, auto-probe cost of single-index sliced dumps (+0.2-0.4 s, L3-007), stall root cause on kvmsvr1.
