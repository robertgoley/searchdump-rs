# searchdump sequential speed suite — es819-plain (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 2.00× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T14:17:34-05:00, finished 2026-11-16T15:08:47-05:00
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

## searchdump × es819-plain (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.968 | 4.115 | 2.09× | 50812 | 88.2 | — | PASS (NO_BASELINE) |
| restore-none | 15.284 | 17.789 | 1.16× | 6543 | 89.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.023 | 12.335 | 6.10× | 49440 | 119.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 15.226 | 17.915 | 1.18× | 6568 | 94.9 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.994 | — | — | 50139 | 137.0 | — | PASS (NO_BASELINE) |
| restore-zstd | 15.027 | — | — | 6654 | 94.0 | — | PASS (NO_BASELINE) |
| copy | 15.162 | 18.018 | 1.19× | 6595 | 143.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.014 | — | — | 49642 | 97.5 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.380 | — | — | 42020 | 97.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.958 | — | — | 51071 | 122.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.211 | — | — | 82554 | 133.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.102 | — | — | 90725 | 145.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.664 | — | — | 150539 | 188.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.644 | — | — | 155329 | 198.2 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.396 † | — | — | 252773 | 250.7 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.946 | 3.884 | 2.00× | 51388 | 98.4 | — | PASS (NO_BASELINE) |
| restore-split-none | 14.982 | 20.974 | 1.40× | 6675 | 93.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.038 | 5.208 | 2.56× | 49062 | 119.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.082 | 21.323 | 1.41× | 6631 | 93.2 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.054 | — | — | 48679 | 125.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 15.202 | — | — | 6578 | 99.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | 0.695 | 27.80× | 39 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.078 † | 0.849 | 10.88× | 13 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | 0.681 | 27.24× | 40 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.253 † | 0.930 | 3.68× | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.971 | 13.297 | 1.21× | 9115 | 94.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.338 | 14.237 | 1.26× | 8820 | 155.1 | — | PASS (NO_BASELINE) |
| dump-small | 1.087 | 2.321 | 2.14× | 91984 | 72.2 | — | PASS (NO_BASELINE) |
| restore-small | 9.299 | 11.969 | 1.29× | 10753 | 69.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.116 | 3.603 | 3.23× | 30 | 250.9 | — | PASS (NO_BASELINE) |
| restore-big | 4.421 | 6.971 | 1.58× | 8 | 441.0 | — | PASS (NO_BASELINE) |
| dump-edge | 2.322 | — | — | 4062 | 327.8 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.404 | — | — | 2142 | 643.5 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.479 | 15.647 | 6.31× | 40343 | 75.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 29.695 | 45.776 | 1.54× | 3368 | 69.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.625 | 4.416 | 7.07× | 159911 | 73.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.914 | 14.313 | 1.44× | 10087 | 70.5 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.368 † | 2.628 | 7.14× | 271601 | 75.0 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.324 | 8.818 | 1.39× | 15814 | 71.6 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1074364 (cap 1048576)

