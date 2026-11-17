# 1.3 work summary (grokbox, local branch `release/1.3`, 2026-11-17)

Branch `release/1.3` is cut from `origin/main` 53bb69a. It is **local only**: nothing is pushed, and there are no
tags, no PRs and no reruns elsewhere.
- Commits: L3-001 to L3-012 (see `git log 53bb69a..`).
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

## Decisions needed from Robert

1. Close 1.3-G as "not a 1.2 regression". Optionally have kvmsvr1 rerun in ABA order, plus `stall_loop.py` for
   the OS 2/3 stalls.
2. 1.3 429 policy: should `retryfirst` + `growback` become the default for parent-breaker 429s, or stay opt-in?
   An alternative is cause-aware handling: never split when "real usage" alone is at or above the limit. This
   reverses part of L2-003.
3. Should the probe floor (`floor` or `configured`) become the default against the 0 MB page budget?
4. Take the L3-008 parent retry fix and the L3-004 log text change into 1.3.
5. Keep the harness DELETE wait and close 1.3-R1.
6. Should Suite A pass `--serverLimits=off` for plain data runs, to compare equal flags?
7. Whether and when to push `release/1.3`.

## What remains

- Settle/re-probe option (not implemented).
- 384m reps.
- R1 pause/memory-pressure variants.
- Per-child uuid logging.
- Measure the auto probe cost in sliced dumps.
- Stall root cause on kvmsvr1.
- Before any default change: a growback info→warn log level and a lowheap counter for splits.
