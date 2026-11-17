# searchdump sequential speed suite — es717-secure (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.90× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T09:06:54-05:00, finished 2026-11-16T10:04:02-05:00
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

## searchdump × es717-secure (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.098 | 3.993 | 1.90× | 47656 | 66.3 | — | PASS (NO_BASELINE) |
| restore-none | 15.056 | 17.611 | 1.17× | 6642 | 99.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.844 | 13.368 | 7.25× | 54232 | 120.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 14.711 | 17.539 | 1.19× | 6798 | 102.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.043 | — | — | 48956 | 95.5 | — | PASS (NO_BASELINE) |
| restore-zstd | 15.110 | — | — | 6618 | 104.8 | — | PASS (NO_BASELINE) |
| copy | 14.720 | 17.334 | 1.18× | 6793 | 138.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.979 | — | — | 50526 | 70.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.926 | — | — | 51934 | 76.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 3.069 | — | — | 32581 | 97.9 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.114 | — | — | 89737 | 112.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.876 | — | — | 53306 | 136.6 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.651 | — | — | 153507 | 175.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 3.705 | — | — | 26993 | 156.5 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.442 † | — | — | 226363 | 258.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.907 | 3.967 | 2.08× | 52441 | 82.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 15.867 | 21.975 | 1.38× | 6302 | 106.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.008 | 5.096 | 2.54× | 49792 | 115.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.806 | 22.422 | 1.42× | 6326 | 116.4 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.958 | — | — | 51074 | 123.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 15.526 | — | — | 6441 | 103.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.693 | 28.87× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.070 † | 0.825 | 11.79× | 14 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | 0.705 | 27.12× | 39 | 26.8 | — | PASS (NO_BASELINE) |
| settings-restore | 0.193 † | 0.888 | 4.60× | 5 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.930 | 14.019 | 1.18× | 8382 | 100.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.286 | 14.127 | 1.15× | 8139 | 130.0 | — | PASS (NO_BASELINE) |
| dump-small | 1.105 | 2.443 | 2.21× | 90482 | 58.1 | — | PASS (NO_BASELINE) |
| restore-small | 8.537 | 11.037 | 1.29× | 11714 | 76.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.907 | 4.499 | 2.36× | 17 | 266.6 | — | PASS (NO_BASELINE) |
| restore-big | 6.722 | 9.245 | 1.38× | 5 | 400.8 | — | PASS (NO_BASELINE) |
| dump-edge | 3.524 | — | — | 2676 | 315.5 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 6.456 | — | — | 1461 | 622.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.899 | 16.398 | 5.66× | 34490 | 80.5 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.055 | 46.799 | 1.56× | 3327 | 79.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.678 | 4.808 | 7.09× | 147409 | 74.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.990 | 15.524 | 1.41× | 9099 | 80.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.383 † | 2.889 | 7.54× | 261066 | 75.0 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.266 | 9.957 | 1.37× | 13763 | 78.8 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1892248 (cap 1048576)

