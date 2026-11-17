# searchdump sequential speed suite — elasticsearch-plain (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T19:01:12-05:00, finished 2026-11-16T19:24:50-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch-plain (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.989 | — | — | 50275 | 89.5 | — | PASS (NO_BASELINE) |
| restore-none | 10.026 | — | — | 9974 | 120.1 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.906 | — | — | 52471 | 108.6 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.172 | — | — | 9831 | 120.9 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.853 | — | — | 53955 | 104.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.461 | — | — | 9560 | 113.4 | — | PASS (NO_BASELINE) |
| copy | 10.914 | — | — | 9162 | 154.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.913 | — | — | 52284 | 90.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.314 | — | — | 43222 | 96.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.918 | — | — | 52150 | 113.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.185 | — | — | 84356 | 142.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.063 | — | — | 94044 | 150.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.630 | — | — | 158852 | 200.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.605 | — | — | 165345 | 230.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.399 † | — | — | 250640 | 256.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.924 | — | — | 51974 | 90.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.629 | — | — | 9408 | 98.2 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.956 | — | — | 51130 | 120.4 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.585 | — | — | 9447 | 98.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.976 | — | — | 50606 | 116.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.626 | — | — | 9411 | 102.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 41 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.073 † | — | — | 14 | 26.8 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.244 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.923 | — | — | 10078 | 119.8 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.681 | — | — | 9363 | 157.1 | — | PASS (NO_BASELINE) |
| dump-small | 1.070 | — | — | 93488 | 62.2 | — | PASS (NO_BASELINE) |
| restore-small | 5.854 | — | — | 17082 | 90.1 | — | PASS (NO_BASELINE) |
| dump-big | 0.861 | — | — | 38 | 356.3 | — | PASS (NO_BASELINE) |
| restore-big | 3.352 | — | — | 10 | 459.3 | — | PASS (NO_BASELINE) |
| dump-edge | 2.088 | — | — | 4518 | 342.4 | — | PASS (NO_BASELINE) |
| restore-edge | 2.900 | — | — | 3252 | 671.6 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.099 | — | — | 47650 | 72.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.673 | — | — | 3749 | 75.4 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.566 | — | — | 176554 | 73.4 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.317 | — | — | 10733 | 79.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.311 † | — | — | 321763 | 73.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.064 | — | — | 16490 | 76.2 | — | PASS (NO_BASELINE) |

