# searchdump sequential speed suite — es717-plain (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.96× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T07:25:55-05:00, finished 2026-11-16T08:17:40-05:00
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

## searchdump × es717-plain (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.937 | 3.801 | 1.96× | 51624 | 93.2 | — | PASS (NO_BASELINE) |
| restore-none | 14.723 | 17.135 | 1.16× | 6792 | 91.2 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.866 | 13.088 | 7.01× | 53591 | 129.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 14.469 | 17.063 | 1.18× | 6911 | 91.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.826 | — | — | 54750 | 115.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 14.868 | — | — | 6726 | 92.8 | — | PASS (NO_BASELINE) |
| copy | 14.883 | 17.365 | 1.17× | 6719 | 156.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.718 | — | — | 58196 | 99.0 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.103 | — | — | 47555 | 92.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.993 | — | — | 50184 | 111.0 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.123 | — | — | 89082 | 135.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.078 | — | — | 92756 | 150.1 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.629 | — | — | 158862 | 204.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.689 | — | — | 145181 | 228.9 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.425 † | — | — | 235320 | 276.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.878 | 3.803 | 2.03× | 53253 | 91.6 | — | PASS (NO_BASELINE) |
| restore-split-none | 16.316 | 21.761 | 1.33× | 6129 | 93.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.921 | 5.080 | 2.64× | 52065 | 121.4 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.970 | 21.980 | 1.38× | 6262 | 95.5 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.955 | — | — | 51163 | 118.1 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 16.090 | — | — | 6215 | 93.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.028 † | 0.676 | 24.14× | 36 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.082 † | 0.807 | 9.84× | 12 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | 0.670 | 26.80× | 40 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.185 † | 0.848 | 4.58× | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.761 | 13.447 | 1.14× | 8502 | 92.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.483 | 13.649 | 1.19× | 8708 | 148.6 | — | PASS (NO_BASELINE) |
| dump-small | 0.993 | 2.149 | 2.16× | 100700 | 76.7 | — | PASS (NO_BASELINE) |
| restore-small | 8.482 | 11.079 | 1.31× | 11790 | 79.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.187 | 3.679 | 3.10× | 28 | 271.3 | — | PASS (NO_BASELINE) |
| restore-big | 6.035 | 8.370 | 1.39× | 6 | 441.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.460 | — | — | 3833 | 363.2 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.805 | — | — | 1625 | 649.7 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.216 | 15.054 | 6.79× | 45121 | 75.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 29.760 | 45.711 | 1.54× | 3360 | 69.7 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.612 | 4.403 | 7.19× | 163488 | 71.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.818 | 15.067 | 1.39× | 9244 | 70.5 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.330 † | 2.688 | 8.15× | 303214 | 71.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.192 | 9.654 | 1.34× | 13904 | 70.3 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1749308 (cap 1048576)

