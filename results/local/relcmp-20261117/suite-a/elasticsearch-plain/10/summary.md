# searchdump sequential speed suite — elasticsearch-plain (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.87× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T17:47:03-05:00, finished 2026-11-16T18:37:24-05:00
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

## searchdump × elasticsearch-plain (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.066 | 3.857 | 1.87× | 48413 | 91.3 | — | PASS (NO_BASELINE) |
| restore-none | 15.437 | 17.897 | 1.16× | 6478 | 94.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.018 | 12.951 | 6.42× | 49565 | 125.5 | — | PASS (NO_BASELINE) |
| restore-gzip | 15.046 | 17.754 | 1.18× | 6646 | 96.4 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.011 | — | — | 49724 | 137.0 | — | PASS (NO_BASELINE) |
| restore-zstd | 15.087 | — | — | 6628 | 101.3 | — | PASS (NO_BASELINE) |
| copy | 13.849 | 17.668 | 1.28× | 7220 | 147.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.975 | — | — | 50638 | 85.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.303 | — | — | 43425 | 87.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.917 | — | — | 52153 | 125.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.205 | — | — | 82995 | 149.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.032 | — | — | 96920 | 176.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.653 | — | — | 153219 | 198.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.598 | — | — | 167311 | 189.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.401 † | — | — | 249618 | 268.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.012 | 3.832 | 1.90× | 49698 | 89.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 14.989 | 20.919 | 1.40× | 6672 | 97.5 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.087 | 5.100 | 2.44× | 47921 | 132.0 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 14.862 | 21.214 | 1.43× | 6729 | 106.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.081 | — | — | 48060 | 130.1 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 14.824 | — | — | 6746 | 100.7 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.655 | 27.29× | 41 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.072 † | 0.786 | 10.92× | 14 | 27.0 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | 0.670 | 26.80× | 40 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.242 † | 0.898 | 3.71× | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.660 | 13.141 | 1.23× | 9381 | 101.0 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.618 | 13.799 | 1.19× | 8607 | 149.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.090 | 2.476 | 2.27× | 91775 | 73.3 | — | PASS (NO_BASELINE) |
| restore-small | 7.668 | 10.027 | 1.31× | 13040 | 67.7 | — | PASS (NO_BASELINE) |
| dump-big | 1.119 | 3.532 | 3.16× | 30 | 347.8 | — | PASS (NO_BASELINE) |
| restore-big | 4.707 | 7.198 | 1.53× | 7 | 441.6 | — | PASS (NO_BASELINE) |
| dump-edge | 2.132 | — | — | 4424 | 325.4 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.657 | — | — | 2025 | 649.2 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.180 | 14.949 | 6.86× | 45874 | 68.3 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.560 | 46.328 | 1.52× | 3272 | 68.9 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.564 | 4.513 | 8.00× | 177228 | 73.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.938 | 14.299 | 1.44× | 10063 | 71.3 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.357 † | 2.608 | 7.31× | 280112 | 69.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.074 | 8.447 | 1.39× | 16464 | 72.1 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1071928 (cap 1048576)

