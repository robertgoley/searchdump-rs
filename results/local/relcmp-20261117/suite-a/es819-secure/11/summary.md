# searchdump sequential speed suite — es819-secure (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T16:51:35-05:00, finished 2026-11-16T17:19:40-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es819-secure (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.370 | — | — | 42190 | 82.7 | — | PASS (NO_BASELINE) |
| restore-none | 10.658 | — | — | 9383 | 102.1 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.240 | — | — | 44637 | 86.6 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.661 | — | — | 9380 | 107.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.282 | — | — | 43822 | 87.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 11.115 | — | — | 8997 | 113.2 | — | PASS (NO_BASELINE) |
| copy | 11.138 | — | — | 8978 | 136.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.261 | — | — | 44223 | 72.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.627 | — | — | 38067 | 71.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.006 | — | — | 49843 | 100.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.432 | — | — | 69811 | 116.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.177 | — | — | 84990 | 132.9 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.757 | — | — | 132145 | 154.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.698 | — | — | 143291 | 220.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.488 † | — | — | 205049 | 236.9 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.334 | — | — | 42846 | 85.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.310 | — | — | 8842 | 100.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.304 | — | — | 43400 | 96.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.004 | — | — | 9088 | 98.9 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.340 | — | — | 42732 | 97.3 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.919 | — | — | 9158 | 105.1 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.031 † | — | — | 32 | 26.8 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.128 † | — | — | 8 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.029 † | — | — | 35 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.396 † | — | — | 2 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.036 | — | — | 9964 | 98.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.685 | — | — | 9359 | 142.3 | — | PASS (NO_BASELINE) |
| dump-small | 1.152 | — | — | 86782 | 72.3 | — | PASS (NO_BASELINE) |
| restore-small | 6.801 | — | — | 14704 | 84.8 | — | PASS (NO_BASELINE) |
| dump-big | 1.214 | — | — | 27 | 288.1 | — | PASS (NO_BASELINE) |
| restore-big | 3.688 | — | — | 9 | 450.6 | — | PASS (NO_BASELINE) |
| dump-edge | 2.740 | — | — | 3442 | 333.1 | — | PASS (NO_BASELINE) |
| restore-edge | 2.991 | — | — | 3153 | 504.9 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.835 | — | — | 35268 | 80.4 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.354 | — | — | 3294 | 81.4 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.722 | — | — | 138528 | 77.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.687 | — | — | 9358 | 77.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.392 † | — | — | 254951 | 76.0 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 8.013 | — | — | 12480 | 77.8 | — | PASS (NO_BASELINE) |

