# searchdump sequential speed suite — os2-secure (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.89× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T02:26:36-05:00, finished 2026-11-17T03:25:45-05:00
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

## searchdump × os2-secure (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.220 | 4.187 | 1.89× | 45042 | 72.5 | — | PASS (NO_BASELINE) |
| restore-none | 20.751 | 22.053 | 1.06× | 4819 | 85.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.300 | 12.963 | 5.64× | 43482 | 97.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 19.926 | 23.471 | 1.18× | 5019 | 86.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.168 | — | — | 46130 | 81.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 19.629 | — | — | 5094 | 84.7 | — | PASS (NO_BASELINE) |
| copy | 19.916 | 22.302 | 1.12× | 5021 | 143.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.947 | — | — | 51359 | 72.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.628 | — | — | 38056 | 68.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.186 | — | — | 45754 | 95.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.760 | — | — | 36238 | 102.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.219 | — | — | 82052 | 150.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.501 | — | — | 66602 | 162.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.769 | — | — | 130013 | 186.4 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.967 | — | — | 103457 | 203.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.034 | 4.065 | 2.00× | 49155 | 72.3 | — | PASS (NO_BASELINE) |
| restore-split-none | 20.607 | 25.868 | 1.26× | 4853 | 92.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.290 | 5.310 | 2.32× | 43664 | 108.2 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 19.493 | 25.873 | 1.33× | 5130 | 95.2 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.098 | — | — | 47653 | 110.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 19.521 | — | — | 5123 | 97.3 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | 0.704 | 28.16× | 40 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.105 † | 0.892 | 8.50× | 10 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | 0.683 | 27.32× | 40 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.251 † | 0.951 | 3.79× | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 12.638 | 15.123 | 1.20× | 7913 | 100.4 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.749 | 14.879 | 1.17× | 7844 | 129.6 | — | PASS (NO_BASELINE) |
| dump-small | 1.160 | 2.497 | 2.15× | 86210 | 49.1 | — | PASS (NO_BASELINE) |
| restore-small | 11.065 | 13.785 | 1.25× | 9038 | 66.5 | — | PASS (NO_BASELINE) |
| dump-big | 1.834 | 4.542 | 2.48× | 18 | 303.1 | — | PASS (NO_BASELINE) |
| restore-big | 5.199 | 7.845 | 1.51× | 6 | 398.6 | — | PASS (NO_BASELINE) |
| dump-edge | 3.496 | — | — | 2698 | 308.4 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.100 | — | — | 1849 | 620.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.779 | 16.357 | 5.89× | 35986 | 71.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 27.011 | 42.843 | 1.59× | 3702 | 79.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.683 | 4.707 | 6.89× | 146388 | 72.2 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.844 | 13.604 | 1.54× | 11308 | 78.8 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.395 † | 2.840 | 7.19× | 252942 | 72.4 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.779 | 8.568 | 1.48× | 17304 | 79.1 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1216836 (cap 1048576)

