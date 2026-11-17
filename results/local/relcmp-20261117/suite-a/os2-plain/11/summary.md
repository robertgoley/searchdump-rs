# searchdump sequential speed suite — os2-plain (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T01:37:26-05:00, finished 2026-11-17T02:01:46-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os2-plain (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.888 | — | — | 52968 | 100.2 | — | PASS (NO_BASELINE) |
| restore-none | 10.977 | — | — | 9110 | 102.3 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.818 | — | — | 54998 | 105.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.671 | — | — | 9371 | 106.2 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.702 | — | — | 58743 | 114.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.647 | — | — | 9392 | 110.4 | — | PASS (NO_BASELINE) |
| copy | 11.243 | — | — | 8894 | 157.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.642 | — | — | 60902 | 91.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.834 | — | — | 54515 | 96.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.940 | — | — | 51559 | 119.3 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.344 | — | — | 42654 | 126.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.019 | — | — | 98129 | 178.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.282 | — | — | 78018 | 179.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.645 | — | — | 154943 | 201.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.787 | — | — | 126988 | 249.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.685 | — | — | 59332 | 109.3 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.221 | — | — | 8912 | 97.5 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.679 | — | — | 59566 | 114.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.654 | — | — | 8581 | 97.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.734 | — | — | 57676 | 115.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.475 | — | — | 8715 | 99.7 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 39 | 27.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.089 † | — | — | 11 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.231 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.414 | — | — | 9602 | 106.7 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.885 | — | — | 9187 | 157.3 | — | PASS (NO_BASELINE) |
| dump-small | 0.973 | — | — | 102751 | 76.1 | — | PASS (NO_BASELINE) |
| restore-small | 5.810 | — | — | 17211 | 88.1 | — | PASS (NO_BASELINE) |
| dump-big | 0.998 | — | — | 33 | 281.4 | — | PASS (NO_BASELINE) |
| restore-big | 3.260 | — | — | 10 | 449.2 | — | PASS (NO_BASELINE) |
| dump-edge | 7.414 | — | — | 1272 | 326.3 | — | PASS (NO_BASELINE) |
| restore-edge | 7.828 | — | — | 1205 | 587.6 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.200 | — | — | 45463 | 78.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 20.664 | — | — | 4839 | 75.7 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.585 | — | — | 170834 | 69.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.290 | — | — | 13717 | 77.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.353 † | — | — | 283612 | 70.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.069 | — | — | 19726 | 74.6 | — | PASS (NO_BASELINE) |

