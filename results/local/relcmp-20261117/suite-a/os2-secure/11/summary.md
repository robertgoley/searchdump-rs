# searchdump sequential speed suite — os2-secure (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T03:25:46-05:00, finished 2026-11-17T03:54:46-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os2-secure (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.453 | — | — | 40774 | 74.4 | — | PASS (NO_BASELINE) |
| restore-none | 12.511 | — | — | 7993 | 100.3 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.077 | — | — | 48143 | 92.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.518 | — | — | 7398 | 102.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.993 | — | — | 50172 | 85.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 13.222 | — | — | 7563 | 104.7 | — | PASS (NO_BASELINE) |
| copy | 13.213 | — | — | 7568 | 123.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.055 | — | — | 48664 | 70.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.378 | — | — | 42061 | 76.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.398 | — | — | 41704 | 113.3 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.775 | — | — | 36031 | 106.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.191 | — | — | 83955 | 141.1 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.488 | — | — | 67198 | 149.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.790 | — | — | 126640 | 187.0 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 1.002 | — | — | 99782 | 229.5 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.102 | — | — | 47581 | 78.9 | — | PASS (NO_BASELINE) |
| restore-split-none | 13.371 | — | — | 7479 | 104.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.905 | — | — | 52490 | 95.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 13.366 | — | — | 7482 | 110.8 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.836 | — | — | 54457 | 89.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 13.514 | — | — | 7400 | 103.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.107 † | — | — | 9 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.261 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.986 | — | — | 8343 | 105.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.864 | — | — | 7774 | 137.1 | — | PASS (NO_BASELINE) |
| dump-small | 1.160 | — | — | 86218 | 50.7 | — | PASS (NO_BASELINE) |
| restore-small | 7.220 | — | — | 13851 | 88.4 | — | PASS (NO_BASELINE) |
| dump-big | 2.007 | — | — | 16 | 268.9 | — | PASS (NO_BASELINE) |
| restore-big | 4.419 | — | — | 8 | 437.5 | — | PASS (NO_BASELINE) |
| dump-edge | 3.381 | — | — | 2789 | 328.8 | — | PASS (NO_BASELINE) |
| restore-edge | 3.271 | — | — | 2883 | 508.2 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.693 | — | — | 37135 | 76.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 22.602 | — | — | 4424 | 84.2 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.729 | — | — | 137220 | 72.0 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.937 | — | — | 12599 | 80.3 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.378 † | — | — | 264258 | 72.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.633 | — | — | 17754 | 81.3 | — | PASS (NO_BASELINE) |

