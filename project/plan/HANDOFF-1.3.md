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

## Robert's decisions (2026-11-17, recorded in L3-014)
| # | decision | where |
|---|---|---|
| 1 (t226u) | 1.3-G stays **open**; retest on kvmsvr1 **after the 1.3 release** (alternating 1.1/1.2/1.3 plus `stall_loop.py`) | FEATURES-TODO 1.3-G |
| 2 | `retryfirst` / `growback` default: **open**. Robert (t227u/t228u) asked for growback tests first, incl. the failed grow-back penalty | L3-015+ (`results/local/L3-0NN-growback.md`) |
| 3 (t221u) | **Floor is the 1.3 default**: sizing from >= 1/4 of the parent-breaker limit; page and in-flight budgets >= 1 MiB (`PAGE_FLOOR_BYTES`); bulk requests >= 1 MiB (`BULK_FLOOR_BYTES`); a 429 split keeps the cap for later bulks >= 1 MiB (the refused request still splits to 1 doc); a 429 page halving stops once at 10 docs (`PAGE_FLOOR_DOCS`). Lower user `--limit`/`--maxBulkBytes` still apply. Escape hatch `SEARCHDUMP_LIMITS_EXPERIMENT=nofloor` | L3-014, src/limits.rs, README, CHANGELOG/RELEASE-NOTES-1.3 drafts |
| 4 (t222u) | **Ship L3-008** (multi parent retry of GET/HEAD/`_refresh`/`_count` on 429/503; false index failure in 1.1.0/1.2.0) and the **L3-004** 429-cause warning wording | CHANGELOG / RELEASE-NOTES-1.3 drafts, FEATURES-TODO 1.3-D+ |
| 5 (t223u) | Keep the L3-002 harness DELETE wait; **1.3-R1 closed** (harness-caused, mitigated, 63 restores 0 failures) | REVIEW-1.3.md, FEATURES-TODO 1.3-R1 |
| 6 (t225u) | Suite A single-index data cases pass `--serverLimits=off` for 1.2+ searchdump binaries (probe not timed); 1.0/1.1/Node unchanged | scripts/perf/suite.py, PERF-PLAN-1.0-1.2.md, FEATURES-TODO 1.3-P |
| 7 | Push of `release/1.3`: **open** | - |

Floor values, reasoning: L3-011 shows OS 3 small-heap bulks accepted at 1.5-4.8 MB right after a split, while 1.2's
never-grow-back cascade ended at 54-390 KB (25-100x more requests). 1 MiB is ~45 pmc docs / ~13 fat docs per bulk,
under every observed accepted size, and equals 1.2's existing lower bound for the bulk cap (`max_content_length - 1 MB`,
min 1 MiB). The 1/4-of-parent-limit sizing floor is the L3-003 `floor` experiment (the relcmp os3/384m dump-fat
probe at 94 % heap gives a 22.8 MB page, ~96 fat docs, instead of 0 MB / 3 docs). Page halving keeps the 1-document
minimum for correctness (callers detect the slice end from the page size), so the floor there is a stop at 10 docs.

## Update 20:45 ET (growback tests, Robert t227u/t228u)
- Commits: L3-014 b36f749 (floor default + decisions 1/4/5/6), L3-015 5727aae (floor reps OS3 256m), L3-016 1cf3d9e
  (CapControl counters, failed grow-back penalty, `bulk size summary` line, grow back-off 8..256, unit tests,
  `scripts/load/growback_reps.py`).
- Running (do NOT start duplicates): `/workspace/rel-1.3-work/chain-gb.sh` (log `chain-gb.log`, rows
  `gb-rows-<case>.jsonl`; OS3 256/384/512m, OS2 256m, ES 8.19 256m; load + multi8; 4 variants x 4 reps), then
  `chain-gb2.sh` (log `chain-gb2.log`; sustained-429: es819 indexing_pressure 2mb with `--serverLimits=off`, 1mb with
  auto, OS3 256m off). The planned "sustained" auto case with 2mb gave no 429 (auto sizes bulks to 1 MiB), hence chain 2.
- Next: `python3 /workspace/rel-1.3-work/gb_table.py` (tables), write `results/local/L3-017-growback.md`, copy reps.json
  from `results/load/l3-016/*` to `results/local/L3-017/`, commit named files, check `podman ps -a` (isolated store).
