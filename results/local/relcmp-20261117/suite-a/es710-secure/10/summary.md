# searchdump sequential speed suite — es710-secure (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE; × faster median 1.92× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T05:00:10-05:00, finished 2026-11-16T06:15:42-05:00
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

## searchdump × es710-secure (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.053 | 4.200 | 2.05× | 48699 | 77.7 | — | PASS (NO_BASELINE) |
| restore-none | 14.496 | 17.148 | 1.18× | 6899 | 91.0 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.047 | 14.294 | 6.98× | 48859 | 102.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 14.252 | 16.932 | 1.19× | 7016 | 100.5 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.022 | — | — | 49451 | 95.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 13.983 | — | — | 7152 | 101.0 | — | PASS (NO_BASELINE) |
| copy | 17.616 | 19.548 | 1.11× | 5677 | 134.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.342 | — | — | 42691 | 76.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 9.722 | — | — | 10286 | 99.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 6.003 | — | — | 16657 | 126.4 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 12.730 | — | — | 7856 | 140.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 2.369 | 4.550 | 1.92× | 42218 | 84.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 17.189 | 23.229 | 1.35× | 5818 | 106.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.310 | 5.782 | 2.50× | 43295 | 108.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 17.048 | 23.538 | 1.38× | 5866 | 100.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.283 | — | — | 43798 | 107.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 17.200 | — | — | 5814 | 100.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.033 † | 0.722 | 21.88× | 30 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.080 † | 0.838 | 10.47× | 12 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.034 † | 0.726 | 21.35× | 29 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.200 † | 0.909 | 4.54× | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.560 | 13.575 | 1.17× | 8650 | 93.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.862 | 14.171 | 1.10× | 7775 | 130.4 | — | PASS (NO_BASELINE) |
| dump-small | 0.942 | 2.571 | 2.73× | 106115 | 50.6 | — | PASS (NO_BASELINE) |
| restore-small | 8.544 | 10.744 | 1.26× | 11704 | 76.5 | — | PASS (NO_BASELINE) |
| dump-big | 2.068 | 4.985 | 2.41× | 16 | 274.0 | — | PASS (NO_BASELINE) |
| restore-big | 6.774 | 9.382 | 1.39× | 5 | 398.5 | — | PASS (NO_BASELINE) |
| dump-edge | 5.096 | — | — | 1851 | 287.1 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 6.059 | — | — | 1557 | 622.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 3.519 | 17.526 | 4.98× | 28416 | 74.6 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.248 | 41.636 | 1.65× | 3961 | 80.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.926 | 5.133 | 5.54× | 107978 | 73.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.014 | 13.588 | 1.51× | 11093 | 78.7 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.511 | 2.982 | 5.84× | 195642 | 74.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.026 | 8.536 | 1.42× | 16594 | 78.6 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1218664 (cap 1048576)

