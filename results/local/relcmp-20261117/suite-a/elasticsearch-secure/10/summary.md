# searchdump sequential speed suite — elasticsearch-secure (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.81× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T19:24:52-05:00, finished 2026-11-16T20:21:51-05:00
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

## searchdump × elasticsearch-secure (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.374 | 4.468 | 1.88× | 42121 | 74.7 | — | PASS (NO_BASELINE) |
| restore-none | 16.299 | 19.343 | 1.19× | 6135 | 100.6 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.438 | 13.139 | 5.39× | 41023 | 114.5 | — | PASS (NO_BASELINE) |
| restore-gzip | 16.067 | 18.179 | 1.13× | 6224 | 95.3 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.328 | — | — | 42962 | 112.1 | — | PASS (NO_BASELINE) |
| restore-zstd | 15.340 | — | — | 6519 | 99.0 | — | PASS (NO_BASELINE) |
| copy | 16.128 | 19.212 | 1.19× | 6200 | 140.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.319 | — | — | 43124 | 78.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.599 | — | — | 38477 | 71.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.097 | — | — | 47680 | 96.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.373 | — | — | 72852 | 114.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.175 | — | — | 85108 | 144.4 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.737 | — | — | 135702 | 160.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.661 | — | — | 151206 | 202.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.472 † | — | — | 211860 | 243.5 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.319 | 4.189 | 1.81× | 43124 | 73.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 16.065 | 22.223 | 1.38× | 6225 | 104.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.429 | 5.360 | 2.21× | 41171 | 104.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.829 | 22.032 | 1.39× | 6317 | 97.0 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.372 | — | — | 42164 | 118.9 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 16.070 | — | — | 6223 | 97.3 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.704 | 29.33× | 42 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.115 † | 0.899 | 7.82× | 9 | 26.7 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.712 | 29.67× | 41 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.369 † | 1.057 | 2.86× | 3 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.999 | 13.889 | 1.16× | 8334 | 100.6 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.399 | 15.114 | 1.22× | 8066 | 138.6 | — | PASS (NO_BASELINE) |
| dump-small | 1.172 | 2.668 | 2.28× | 85297 | 62.2 | — | PASS (NO_BASELINE) |
| restore-small | 8.085 | 10.481 | 1.30× | 12369 | 71.4 | — | PASS (NO_BASELINE) |
| dump-big | 1.332 | 3.976 | 2.98× | 25 | 298.7 | — | PASS (NO_BASELINE) |
| restore-big | 5.029 | 7.763 | 1.54× | 7 | 398.8 | — | PASS (NO_BASELINE) |
| dump-edge | 2.623 | — | — | 3596 | 320.4 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.284 | — | — | 1785 | 620.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.567 | 15.994 | 6.23× | 38948 | 77.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 34.071 | 50.106 | 1.47× | 2935 | 77.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.650 | 4.632 | 7.13× | 153812 | 77.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 11.487 | 15.650 | 1.36× | 8706 | 78.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.350 † | 2.878 | 8.22× | 285843 | 74.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.049 | 9.604 | 1.36× | 14186 | 80.5 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1890684 (cap 1048576)

