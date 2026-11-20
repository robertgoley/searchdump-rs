# 1.3 work summary (grokbox, local branch `release/1.3`, 2026-11-17)

Branch `release/1.3` is cut from `origin/main` 53bb69a. It was pushed to origin as its own branch on 2026-11-17 (decision 7). It is not merged, and there are no tags and no
PRs.
- Commits: L3-001 to L3-018 (see `git log 53bb69a..`).
- Binaries: release assets 1.1.0 (searchdump 06233a2a, the same build as kvmsvr1) and 1.2.0 (e0344bbe), both
  sha256-verified.
- Experiment builds: `bin/l3exp*` in `/workspace/rel-1.3-work` (scratch).
- Checks: lib tests (312), multi tests, perf selftest (32), clippy `-D warnings` and `fmt --check` all pass.

## 1.3-G: OS3 plain 1.2 slowdown with equal flags (P1)

**Not a 1.2 regression.**
- On one OS 3.9.0 plain engine, interleaved A/B gives 1.2/1.1 = 0.91-1.08. For example, multi-restore-p8 takes
  0.771 s on 1.2 and 0.776 s on 1.1. The HTTP request traces are identical (no probe, same counts) (L3-001).
- A fresh full 1.2.0 os3-plain Suite A pass was normal (L3-002).
- A same-host 1.1.0 pass gives a median 1.2/1.1 of 1.00 over 37 cases (L3-007).
- The kvmsvr1 pass-12 slowdown started mid-pass (05:55 ET) and also hit code paths 1.2 never touched, so it is
  engine or host state there.
- Side finding: 1.2 single-index sliced dumps are +0.2-0.4 s, probably the auto probe, because Suite A plain data
  runs do not pass `--serverLimits=off`. This is not measured.

**Stalls** (L3-010 data analysis, L3-012 tracing):
- On kvmsvr1 there were 10 of 8,896 runs, all on OS 2/3. They add +12 s at p8 and +18-22 s at p1, and hit Node,
  1.1.0 and 1.2.0 alike.
- On grokbox they were not reproduced: 60 traced restores on OS3 and OS2, worst run 1.6x its median, no old GC,
  no rejections, no pending tasks.
- The cause is on the host or engine side on kvmsvr1. The next step is to run `stall_loop.py` there.

## 1.3-D+ / C / E: OpenSearch 3 small-heap collapse (P1), L3-003/004/006/011

**Cause.** The OS3 real-memory **parent circuit breaker** trips on uncollected garbage: "real usage" alone is at or
above the limit. 1.2's L2-003 handling then halves the bulk and **never grows back**. This cascades from
6.4-16 MB down to 54-390 KB, 3-13x slower.
- It also happens with `--serverLimits=off`.
- 1.1 retried at the same size and recovered.
- OS 2 and ES 9.1.2 never trip.
- The relcmp `retries=0` happened because splits logged no "retrying in" and no body. L3-004 now names the cause.

**OS3 256m pmc load, 4 interleaved reps per variant** (median wall time):

| variant | median |
|---|---|
| 1.1.0 | 4.7 s |
| 1.2.0 auto | 16.7 s (3.8 splits/run) |
| 1.2.0 off | 8.7 s |
| retryfirst | 9.2 s (0 splits) |
| **retryfirst+growback** | **6.7 s** |
| floor+retryfirst+growback | 8.2 s |

`configured` did not help loads. `floor` matters only for the probe/page side.

All experiments are opt-in through `SEARCHDUMP_LIMITS_EXPERIMENT` and the default is unchanged.

## Multi parent 429 bug, L3-008

The parent's 0-byte `POST /idx/_refresh` got a breaker 429 and failed a fully written index (OS3 256m). The same
bug is in 1.1.0 and 1.2.0. Fix: retry read-only parent calls (GET/HEAD, `_refresh`, `_count`) on 429/503, up to 6
times with back-off from 250 ms to 4 s. Unit-tested.

## 1.3-R1: ES 8.6.1 plain multi-restore-p8 x20 (P2), L3-002/009

**0 failures in 63 restores.**

| harness and tool | median | max |
|---|---|---|
| old harness, 1.1.0 | 1.276 s | |
| old harness, 1.2.0 | 1.361 s | 5.39 s |
| new harness, 1.2.0 | 1.820 s | |

The new harness waits for each DELETE and logs it: 336 of 336 DELETEs were applied before the first poll.
REVIEW-1.3.md is set to "mitigated". The pause/memory-pressure variants were not run.

## Reporting (P3), L3-002

`scripts/perf/relcmp_report.py` produces `results/local/relcmp-20261117/REPORT-EXTRA.md`. It includes:
- a binary SHA header;
- the group headline (it reproduces 1.89/2.04/2.01x);
- low-memory timing tables;
- all Suite C 1-core/4-core rows.

## Robert's decisions (2026-11-17) and follow-up work (L3-014..018)

1. 1.3-G stays open; retest on kvmsvr1 after the 1.3 release (t226u).
2. `retryfirst` and `growback` stay opt-in (t229u). Grow-back matrix: `L3-017-growback.md`. A failed grow costs
   about 2 s; growback is 1.51x slower than the floor on OS3 256m load and +43 % under a sustained 429.
3. The floor is the default (t221u, kept as tested in L3-017 by t229u). See L3-014, L3-015 and the L3-017 worker-share
   fix; values are in `project/plan/HANDOFF-1.3.md` section 3.
4. L3-008 and L3-004 ship in 1.3 (t222u).
5. Harness DELETE wait kept; 1.3-R1 closed (t223u).
6. Suite A single-index data cases pass `--serverLimits=off` for 1.2+ (t225u).
7. `release/1.3` pushed to origin as its own branch (t229u).

The pickup document is `project/plan/HANDOFF-1.3.md`.

## What remains

- Settle/re-probe option (not implemented).
- 384m reps.
- R1 pause/memory-pressure variants.
- Per-child uuid logging.
- Measure the auto probe cost in sliced dumps.
- Stall root cause on kvmsvr1.
- Before any default change: a growback info→warn log level and a lowheap counter for splits.
