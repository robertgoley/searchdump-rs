# searchdump sequential speed suite — elasticsearch8-plain (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.91× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T10:58:50-05:00, finished 2026-11-16T11:47:30-05:00
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

## searchdump × elasticsearch8-plain (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.029 | 3.883 | 1.91× | 49278 | 89.9 | — | PASS (NO_BASELINE) |
| restore-none | 12.863 | 15.220 | 1.18× | 7774 | 98.2 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.920 | 12.916 | 6.73× | 52082 | 129.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 12.892 | 15.234 | 1.18× | 7757 | 90.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.922 | — | — | 52016 | 121.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.828 | — | — | 7796 | 90.3 | — | PASS (NO_BASELINE) |
| copy | 12.994 | 15.257 | 1.17× | 7696 | 138.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.933 | — | — | 51741 | 91.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.214 | — | — | 45159 | 97.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.998 | — | — | 50058 | 114.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.169 | — | — | 85530 | 142.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.056 | — | — | 94681 | 144.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.643 | — | — | 155420 | 196.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.608 | — | — | 164541 | 217.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.378 † | — | — | 264241 | 258.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.883 | 3.660 | 1.94× | 53106 | 97.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.683 | 18.502 | 1.46× | 7884 | 91.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.842 | 5.033 | 2.73× | 54280 | 110.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.474 | 18.830 | 1.51× | 8016 | 95.5 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.866 | — | — | 53585 | 115.0 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.535 | — | — | 7977 | 96.2 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.030 † | 0.674 | 22.47× | 34 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.079 † | 0.781 | 9.89× | 13 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | 0.667 | 26.68× | 39 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.234 † | 0.888 | 3.79× | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.265 | 12.072 | 1.18× | 9742 | 103.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.280 | 12.343 | 1.20× | 9728 | 147.4 | — | PASS (NO_BASELINE) |
| dump-small | 1.000 | 2.217 | 2.22× | 99958 | 67.7 | — | PASS (NO_BASELINE) |
| restore-small | 7.674 | 9.941 | 1.30× | 13030 | 60.5 | — | PASS (NO_BASELINE) |
| dump-big | 1.130 | 3.674 | 3.25× | 29 | 325.4 | — | PASS (NO_BASELINE) |
| restore-big | 4.392 | 6.699 | 1.53× | 8 | 441.2 | — | PASS (NO_BASELINE) |
| dump-edge | 2.704 | — | — | 3488 | 379.3 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.708 | — | — | 2003 | 639.4 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.667 | 15.060 | 5.65× | 37490 | 73.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.830 | 46.594 | 1.51× | 3244 | 69.5 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.693 | 4.377 | 6.32× | 144202 | 73.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.722 | 14.027 | 1.44× | 10286 | 66.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.378 † | 2.598 | 6.87× | 264451 | 71.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.436 | 8.763 | 1.36× | 15536 | 70.4 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1193228 (cap 1048576)

