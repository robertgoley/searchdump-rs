# searchdump sequential speed suite — es819-secure (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.77× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T15:55:16-05:00, finished 2026-11-16T16:51:34-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `fc3ceb708a80…`
- searchdump-multi sha256 `ec08baf5c304…`
- Node v22.22.1

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es819-secure (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.389 | 4.223 | 1.77× | 41854 | 80.0 | — | PASS (NO_BASELINE) |
| restore-none | 15.169 | 17.623 | 1.16× | 6592 | 98.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.354 | 13.130 | 5.58× | 42476 | 101.6 | — | PASS (NO_BASELINE) |
| restore-gzip | 14.962 | 17.797 | 1.19× | 6684 | 96.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.322 | — | — | 43066 | 119.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 15.443 | — | — | 6475 | 101.5 | — | PASS (NO_BASELINE) |
| copy | 15.438 | 18.655 | 1.21× | 6477 | 138.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.248 | — | — | 44487 | 71.1 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.585 | — | — | 38686 | 76.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.061 | — | — | 48511 | 120.0 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.398 | — | — | 71516 | 104.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.100 | — | — | 90883 | 164.7 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.751 | — | — | 133141 | 154.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.653 | — | — | 153221 | 227.2 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.452 † | — | — | 221428 | 238.7 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.328 | 4.231 | 1.82× | 42960 | 79.1 | — | PASS (NO_BASELINE) |
| restore-split-none | 15.088 | 21.401 | 1.42× | 6628 | 102.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.289 | 5.320 | 2.32× | 43679 | 111.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.142 | 20.954 | 1.38× | 6604 | 106.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.268 | — | — | 44099 | 115.9 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 14.990 | — | — | 6671 | 102.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | 0.684 | 26.31× | 39 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.116 † | 0.910 | 7.84× | 9 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.027 † | 0.703 | 26.04× | 38 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.382 † | 1.052 | 2.75× | 3 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.960 | 13.572 | 1.13× | 8361 | 95.8 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.870 | 14.546 | 1.23× | 8424 | 140.9 | — | PASS (NO_BASELINE) |
| dump-small | 1.160 | 2.504 | 2.16× | 86194 | 62.3 | — | PASS (NO_BASELINE) |
| restore-small | 9.601 | 11.879 | 1.24× | 10416 | 68.4 | — | PASS (NO_BASELINE) |
| dump-big | 1.318 | 3.844 | 2.92× | 25 | 255.8 | — | PASS (NO_BASELINE) |
| restore-big | 4.599 | 7.276 | 1.58× | 7 | 398.4 | — | PASS (NO_BASELINE) |
| dump-edge | 2.535 | — | — | 3720 | 340.9 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.931 | — | — | 1913 | 620.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.503 | 16.249 | 6.49× | 39954 | 78.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 34.243 | 50.177 | 1.47× | 2920 | 80.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.619 | 4.647 | 7.51× | 161467 | 75.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 11.411 | 16.057 | 1.41× | 8764 | 80.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.352 † | 2.850 | 8.10× | 283880 | 77.8 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.317 | 9.844 | 1.35× | 13667 | 80.0 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1214784 (cap 1048576)

