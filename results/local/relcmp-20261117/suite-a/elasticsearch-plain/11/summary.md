# searchdump sequential speed suite — elasticsearch-plain (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T18:37:24-05:00, finished 2026-11-16T19:01:11-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch-plain (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.948 | — | — | 51346 | 87.6 | — | PASS (NO_BASELINE) |
| restore-none | 9.937 | — | — | 10064 | 116.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.804 | — | — | 55429 | 97.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.627 | — | — | 9410 | 115.2 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.865 | — | — | 53606 | 105.7 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.296 | — | — | 9712 | 111.3 | — | PASS (NO_BASELINE) |
| copy | 10.673 | — | — | 9370 | 147.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.829 | — | — | 54686 | 84.1 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.216 | — | — | 45117 | 99.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.900 | — | — | 52626 | 109.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.207 | — | — | 82881 | 152.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.060 | — | — | 94351 | 152.5 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.630 | — | — | 158614 | 190.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.623 | — | — | 160482 | 178.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.400 † | — | — | 250116 | 264.6 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.861 | — | — | 53745 | 103.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.801 | — | — | 9259 | 99.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.859 | — | — | 53806 | 113.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.841 | — | — | 9224 | 103.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.838 | — | — | 54400 | 108.2 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.515 | — | — | 9510 | 102.1 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.072 † | — | — | 14 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.249 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.676 | — | — | 10334 | 107.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.656 | — | — | 9384 | 164.3 | — | PASS (NO_BASELINE) |
| dump-small | 1.106 | — | — | 90430 | 65.9 | — | PASS (NO_BASELINE) |
| restore-small | 5.925 | — | — | 16877 | 89.3 | — | PASS (NO_BASELINE) |
| dump-big | 0.904 | — | — | 36 | 333.9 | — | PASS (NO_BASELINE) |
| restore-big | 3.600 | — | — | 9 | 447.2 | — | PASS (NO_BASELINE) |
| dump-edge | 2.197 | — | — | 4293 | 320.2 | — | PASS (NO_BASELINE) |
| restore-edge | 2.740 | — | — | 3442 | 587.5 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.035 | — | — | 49135 | 76.0 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.495 | — | — | 3774 | 77.4 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.552 | — | — | 181215 | 77.9 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.073 | — | — | 11022 | 78.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.321 † | — | — | 311974 | 72.0 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.417 | — | — | 13482 | 77.2 | — | PASS (NO_BASELINE) |

