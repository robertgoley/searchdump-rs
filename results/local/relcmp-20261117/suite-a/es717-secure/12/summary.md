# searchdump sequential speed suite — es717-secure (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T10:31:22-05:00, finished 2026-11-16T10:58:49-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es717-secure (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.266 | — | — | 44138 | 71.2 | — | PASS (NO_BASELINE) |
| restore-none | 10.481 | — | — | 9541 | 98.0 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.151 | — | — | 46489 | 83.2 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.510 | — | — | 9515 | 96.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.256 | — | — | 44318 | 90.2 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.306 | — | — | 9703 | 104.0 | — | PASS (NO_BASELINE) |
| copy | 11.196 | — | — | 8932 | 134.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.264 | — | — | 44171 | 72.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.493 | — | — | 40107 | 75.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.186 | — | — | 45737 | 92.9 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.290 | — | — | 77548 | 105.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.136 | — | — | 88061 | 135.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.727 | — | — | 137614 | 171.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.714 | — | — | 140150 | 205.7 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.462 † | — | — | 216579 | 267.4 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.241 | — | — | 44632 | 80.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.647 | — | — | 8586 | 101.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.941 | — | — | 51513 | 97.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.551 | — | — | 8657 | 103.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.259 | — | — | 44266 | 96.2 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.517 | — | — | 8683 | 104.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.078 † | — | — | 13 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.197 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.195 | — | — | 9808 | 102.4 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.711 | — | — | 9336 | 134.8 | — | PASS (NO_BASELINE) |
| dump-small | 1.074 | — | — | 93072 | 56.8 | — | PASS (NO_BASELINE) |
| restore-small | 6.452 | — | — | 15499 | 84.9 | — | PASS (NO_BASELINE) |
| dump-big | 1.700 | — | — | 19 | 268.9 | — | PASS (NO_BASELINE) |
| restore-big | 5.748 | — | — | 6 | 440.6 | — | PASS (NO_BASELINE) |
| dump-edge | 3.513 | — | — | 2684 | 296.6 | — | PASS (NO_BASELINE) |
| restore-edge | 4.941 | — | — | 1909 | 574.6 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.781 | — | — | 35964 | 80.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.325 | — | — | 3799 | 78.5 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.676 | — | — | 147963 | 76.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.269 | — | — | 9738 | 75.5 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.402 † | — | — | 248958 | 76.9 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.925 | — | — | 14441 | 77.3 | — | PASS (NO_BASELINE) |

