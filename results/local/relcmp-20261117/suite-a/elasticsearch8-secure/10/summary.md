# searchdump sequential speed suite — elasticsearch8-secure (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.79× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T12:33:13-05:00, finished 2026-11-16T13:25:42-05:00
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

## searchdump × elasticsearch8-secure (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.301 | 4.113 | 1.79× | 43469 | 69.4 | — | PASS (NO_BASELINE) |
| restore-none | 13.104 | 15.329 | 1.17× | 7631 | 97.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.112 | 13.418 | 6.35× | 47353 | 99.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.151 | 16.034 | 1.22× | 7604 | 105.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.130 | — | — | 46941 | 105.7 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.999 | — | — | 7693 | 92.5 | — | PASS (NO_BASELINE) |
| copy | 13.082 | 15.817 | 1.21× | 7644 | 138.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.077 | — | — | 48156 | 80.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.497 | — | — | 40055 | 78.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.114 | — | — | 47309 | 92.4 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.223 | — | — | 81786 | 109.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.148 | — | — | 87118 | 143.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.719 | — | — | 139100 | 165.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.661 | — | — | 151361 | 245.0 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.426 † | — | — | 234898 | 244.9 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.109 | 3.983 | 1.89× | 47425 | 82.5 | — | PASS (NO_BASELINE) |
| restore-split-none | 13.175 | 19.541 | 1.48× | 7590 | 94.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.236 | 5.197 | 2.32× | 44715 | 103.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 13.081 | 19.335 | 1.48× | 7645 | 94.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.111 | — | — | 47366 | 119.7 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 13.202 | — | — | 7575 | 98.8 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | 0.701 | 28.04× | 40 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.070 † | 0.791 | 11.30× | 14 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.696 | 29.00× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.230 † | 0.921 | 4.00× | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.501 | 12.384 | 1.18× | 9523 | 101.6 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.541 | 12.620 | 1.20× | 9487 | 134.4 | — | PASS (NO_BASELINE) |
| dump-small | 1.055 | 2.397 | 2.27× | 94763 | 68.3 | — | PASS (NO_BASELINE) |
| restore-small | 7.914 | 10.084 | 1.27× | 12636 | 70.8 | — | PASS (NO_BASELINE) |
| dump-big | 1.405 | 3.968 | 2.82× | 24 | 308.3 | — | PASS (NO_BASELINE) |
| restore-big | 4.590 | 7.310 | 1.59× | 7 | 398.7 | — | PASS (NO_BASELINE) |
| dump-edge | 2.899 | — | — | 3254 | 323.9 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.037 | — | — | 1872 | 620.5 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.516 | 15.859 | 6.30× | 39750 | 77.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.734 | 46.459 | 1.51× | 3254 | 77.4 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.647 | 4.776 | 7.38× | 154454 | 71.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.140 | 14.411 | 1.42× | 9862 | 80.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.375 † | 2.762 | 7.37× | 266973 | 74.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.340 | 8.596 | 1.36× | 15774 | 77.0 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1218036 (cap 1048576)

