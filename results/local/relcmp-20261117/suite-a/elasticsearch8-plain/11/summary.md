# searchdump sequential speed suite — elasticsearch8-plain (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T11:47:31-05:00, finished 2026-11-16T12:10:11-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch8-plain (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.909 | — | — | 52383 | 92.2 | — | PASS (NO_BASELINE) |
| restore-none | 9.160 | — | — | 10917 | 110.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.755 | — | — | 56985 | 112.2 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.104 | — | — | 10984 | 100.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.885 | — | — | 53047 | 106.9 | — | PASS (NO_BASELINE) |
| restore-zstd | 9.075 | — | — | 11019 | 112.4 | — | PASS (NO_BASELINE) |
| copy | 9.617 | — | — | 10398 | 147.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.845 | — | — | 54201 | 95.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.234 | — | — | 44754 | 96.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.859 | — | — | 53793 | 114.9 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.123 | — | — | 89081 | 140.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 0.986 | — | — | 101428 | 160.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.630 | — | — | 158658 | 188.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.574 | — | — | 174353 | 224.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.375 † | — | — | 266386 | 271.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.801 | — | — | 55525 | 94.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 9.311 | — | — | 10740 | 101.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.613 | — | — | 61983 | 103.6 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 9.536 | — | — | 10487 | 101.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.773 | — | — | 56409 | 108.2 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 9.648 | — | — | 10365 | 100.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.072 † | — | — | 14 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | — | — | 41 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.240 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 8.757 | — | — | 11420 | 115.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.136 | — | — | 10946 | 157.3 | — | PASS (NO_BASELINE) |
| dump-small | 0.995 | — | — | 100541 | 69.7 | — | PASS (NO_BASELINE) |
| restore-small | 5.770 | — | — | 17330 | 79.3 | — | PASS (NO_BASELINE) |
| dump-big | 0.936 | — | — | 35 | 280.3 | — | PASS (NO_BASELINE) |
| restore-big | 3.241 | — | — | 10 | 449.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.468 | — | — | 3821 | 338.7 | — | PASS (NO_BASELINE) |
| restore-edge | 2.669 | — | — | 3533 | 587.0 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.189 | — | — | 45677 | 71.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.168 | — | — | 3822 | 76.9 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.560 | — | — | 178554 | 72.2 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.093 | — | — | 10998 | 77.6 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.320 † | — | — | 312966 | 71.9 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.309 | — | — | 13681 | 76.7 | — | PASS (NO_BASELINE) |

