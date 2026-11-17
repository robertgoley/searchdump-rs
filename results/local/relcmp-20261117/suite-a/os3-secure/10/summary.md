# searchdump sequential speed suite — os3-secure (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.94× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T06:10:06-05:00, finished 2026-11-17T07:13:43-05:00
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

## searchdump × os3-secure (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 3.084 | 4.065 | 1.32× | 32421 | 70.3 | — | PASS (NO_BASELINE) |
| restore-none | 22.116 | 23.918 | 1.08× | 4522 | 92.6 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.088 | 13.212 | 6.33× | 47885 | 110.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 22.677 | 23.505 | 1.04× | 4410 | 88.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.046 | — | — | 48874 | 104.1 | — | PASS (NO_BASELINE) |
| restore-zstd | 22.206 | — | — | 4503 | 96.9 | — | PASS (NO_BASELINE) |
| copy | 21.976 | 23.954 | 1.09× | 4550 | 138.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.016 | — | — | 49597 | 66.9 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.487 | — | — | 40206 | 68.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.321 | — | — | 43089 | 100.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.766 | — | — | 36149 | 100.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.214 | — | — | 82361 | 158.6 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.500 | — | — | 66686 | 160.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.716 | — | — | 139616 | 223.0 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.953 | — | — | 104895 | 235.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.009 | 3.896 | 1.94× | 49781 | 86.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 21.937 | 29.613 | 1.35× | 4558 | 92.2 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.933 | 5.066 | 2.62× | 51737 | 120.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 21.676 | 28.265 | 1.30× | 4613 | 99.1 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.059 | — | — | 48561 | 110.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 22.690 | — | — | 4407 | 95.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | 0.705 | 28.20× | 41 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.103 † | 0.912 | 8.85× | 10 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.710 | 29.58× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.248 † | 0.943 | 3.80× | 4 | 26.7 | — | PASS (NO_BASELINE) |
| restore-norefresh | 14.089 | 15.684 | 1.11× | 7098 | 95.7 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.926 | 14.841 | 1.15× | 7737 | 131.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.132 | 2.386 | 2.11× | 88304 | 60.6 | — | PASS (NO_BASELINE) |
| restore-small | 13.106 | 15.956 | 1.22× | 7630 | 64.3 | — | PASS (NO_BASELINE) |
| dump-big | 1.871 | 4.420 | 2.36× | 18 | 242.3 | — | PASS (NO_BASELINE) |
| restore-big | 5.431 | 8.248 | 1.52× | 6 | 398.7 | — | PASS (NO_BASELINE) |
| dump-edge | 3.439 | — | — | 2742 | 308.1 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.025 | — | — | 1877 | 620.2 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.630 | 16.109 | 6.13× | 38029 | 74.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 29.215 | 49.159 | 1.68× | 3423 | 77.8 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.993 | 4.761 | 4.79× | 100701 | 75.0 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.774 | 13.787 | 1.57× | 11398 | 80.8 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.427 † | 2.897 | 6.78× | 234284 | 74.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.019 | 20.247 | 3.36× | 16614 | 78.9 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1886860 (cap 1048576)

