# searchdump sequential speed suite — elasticsearch8-secure (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T13:51:24-05:00, finished 2026-11-16T14:17:33-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch8-secure (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.256 | — | — | 44322 | 66.4 | — | PASS (NO_BASELINE) |
| restore-none | 9.707 | — | — | 10302 | 100.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.355 | — | — | 42462 | 75.7 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.728 | — | — | 10280 | 110.1 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.221 | — | — | 45027 | 92.2 | — | PASS (NO_BASELINE) |
| restore-zstd | 9.656 | — | — | 10356 | 107.2 | — | PASS (NO_BASELINE) |
| copy | 10.090 | — | — | 9911 | 129.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.290 | — | — | 43670 | 70.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.533 | — | — | 39482 | 79.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.011 | — | — | 49726 | 111.4 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.318 | — | — | 75900 | 110.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.118 | — | — | 89416 | 151.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.777 | — | — | 128701 | 175.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.673 | — | — | 148516 | 233.7 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.482 † | — | — | 207646 | 241.5 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.196 | — | — | 45543 | 74.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.263 | — | — | 9743 | 106.2 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.143 | — | — | 46655 | 90.7 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.231 | — | — | 9775 | 107.1 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.208 | — | — | 45292 | 104.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.347 | — | — | 9664 | 113.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.028 † | — | — | 36 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.077 † | — | — | 13 | 26.8 | — | PASS (NO_BASELINE) |
| settings-dump | 0.028 † | — | — | 36 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.237 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.323 | — | — | 10726 | 104.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.939 | — | — | 10062 | 143.1 | — | PASS (NO_BASELINE) |
| dump-small | 1.157 | — | — | 86461 | 64.7 | — | PASS (NO_BASELINE) |
| restore-small | 5.936 | — | — | 16846 | 96.9 | — | PASS (NO_BASELINE) |
| dump-big | 1.183 | — | — | 28 | 292.7 | — | PASS (NO_BASELINE) |
| restore-big | 3.697 | — | — | 9 | 460.7 | — | PASS (NO_BASELINE) |
| dump-edge | 2.865 | — | — | 3292 | 328.6 | — | PASS (NO_BASELINE) |
| restore-edge | 3.486 | — | — | 2705 | 587.8 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.477 | — | — | 40365 | 73.0 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.762 | — | — | 3737 | 77.8 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.644 | — | — | 155381 | 78.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.213 | — | — | 10855 | 73.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.378 † | — | — | 264414 | 75.1 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.052 | — | — | 16523 | 77.0 | — | PASS (NO_BASELINE) |

