# searchdump sequential speed suite — os3-plain (3.9.0)

**Results:** 37 PASS, 0 FAIL, 0 skipped/not-run, 37 NO_BASELINE

- git `53bb69ad01b372f9c2fff67372b638b73529a4bc`
- started 2026-11-17T16:21:58-05:00, finished 2026-11-17T16:29:26-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os3-plain (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 0.632 | — | — | 158121 | 70.0 | — | PASS (NO_BASELINE) |
| restore-none | 3.866 | — | — | 25864 | 107.2 | — | PASS (NO_BASELINE) |
| dump-gzip | 0.591 | — | — | 169166 | 82.5 | — | PASS (NO_BASELINE) |
| restore-gzip | 3.733 | — | — | 26791 | 123.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 0.556 | — | — | 179726 | 103.5 | — | PASS (NO_BASELINE) |
| restore-zstd | 3.693 | — | — | 27078 | 124.4 | — | PASS (NO_BASELINE) |
| copy | 3.825 | — | — | 26143 | 154.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 0.793 | — | — | 126045 | 72.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.117 | — | — | 89530 | 64.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.019 | — | — | 98088 | 104.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 0.783 | — | — | 127664 | 112.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 0.442 † | — | — | 226074 | 152.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.426 † | — | — | 234674 | 162.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.332 † | — | — | 301140 | 184.5 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.332 † | — | — | 301320 | 200.4 | — | PASS (NO_BASELINE) |
| dump-split-none | 0.581 | — | — | 172125 | 88.3 | — | PASS (NO_BASELINE) |
| restore-split-none | 4.343 | — | — | 23028 | 104.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 0.571 | — | — | 175149 | 96.2 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 4.369 | — | — | 22888 | 104.1 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 0.558 | — | — | 179354 | 95.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 4.338 | — | — | 23052 | 104.2 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.014 † | — | — | 73 | 24.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.024 † | — | — | 42 | 24.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.011 † | — | — | 89 | 24.0 | — | PASS (NO_BASELINE) |
| settings-restore | 0.048 † | — | — | 21 | 24.1 | — | PASS (NO_BASELINE) |
| dump-small | 0.338 † | — | — | 295739 | 56.4 | — | PASS (NO_BASELINE) |
| restore-small | 1.763 | — | — | 56726 | 83.9 | — | PASS (NO_BASELINE) |
| dump-big | 0.502 | — | — | 66 | 280.1 | — | PASS (NO_BASELINE) |
| restore-big | 1.811 | — | — | 18 | 440.5 | — | PASS (NO_BASELINE) |
| dump-edge | 1.345 | — | — | 7014 | 413.6 | — | PASS (NO_BASELINE) |
| restore-edge | 1.326 | — | — | 7110 | 523.8 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 0.903 | — | — | 110779 | 72.3 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 6.440 | — | — | 15528 | 75.9 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.331 † | — | — | 301664 | 60.5 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 1.965 | — | — | 50884 | 54.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.240 † | — | — | 416433 | 70.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 1.312 | — | — | 76202 | 68.4 | — | PASS (NO_BASELINE) |

