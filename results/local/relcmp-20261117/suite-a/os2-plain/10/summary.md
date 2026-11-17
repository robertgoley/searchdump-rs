# searchdump sequential speed suite — os2-plain (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 2.00× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T00:46:24-05:00, finished 2026-11-17T01:37:25-05:00
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

## searchdump × os2-plain (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.720 | 3.805 | 2.21× | 58141 | 86.9 | — | PASS (NO_BASELINE) |
| restore-none | 16.313 | 19.771 | 1.21× | 6130 | 89.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.827 | 13.367 | 7.32× | 54736 | 118.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 17.075 | 18.131 | 1.06× | 5857 | 87.2 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.686 | — | — | 59312 | 134.9 | — | PASS (NO_BASELINE) |
| restore-zstd | 16.264 | — | — | 6148 | 95.4 | — | PASS (NO_BASELINE) |
| copy | 16.105 | 19.112 | 1.19× | 6209 | 147.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.672 | — | — | 59800 | 89.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.991 | — | — | 50219 | 87.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.004 | — | — | 49906 | 121.0 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.475 | — | — | 40398 | 123.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.085 | — | — | 92138 | 152.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.334 | — | — | 74963 | 163.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.689 | — | — | 145187 | 212.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.849 | — | — | 117849 | 240.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.808 | 3.610 | 2.00× | 55322 | 101.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 16.328 | 22.268 | 1.36× | 6124 | 101.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.765 | 4.884 | 2.77× | 56646 | 126.7 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 15.962 | 22.451 | 1.41× | 6265 | 90.8 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.758 | — | — | 56888 | 139.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 16.114 | — | — | 6206 | 95.8 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.670 | 27.92× | 41 | 27.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.084 † | 0.818 | 9.74× | 12 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.672 | 28.00× | 42 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.219 † | 0.887 | 4.05× | 5 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.656 | 13.580 | 1.17× | 8579 | 92.0 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.792 | 13.830 | 1.17× | 8480 | 146.8 | — | PASS (NO_BASELINE) |
| dump-small | 1.043 | 2.310 | 2.21× | 95897 | 69.3 | — | PASS (NO_BASELINE) |
| restore-small | 8.252 | 10.335 | 1.25× | 12118 | 62.3 | — | PASS (NO_BASELINE) |
| dump-big | 1.173 | 3.723 | 3.17× | 28 | 297.6 | — | PASS (NO_BASELINE) |
| restore-big | 4.392 | 6.707 | 1.53× | 8 | 441.6 | — | PASS (NO_BASELINE) |
| dump-edge | 2.368 | — | — | 3982 | 335.5 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 9.789 | — | — | 963 | 649.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.363 | 14.991 | 6.34× | 42320 | 72.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 24.163 | 38.721 | 1.60× | 4139 | 74.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.577 | 4.398 | 7.62× | 173250 | 82.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.677 | 12.148 | 1.58× | 13026 | 67.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.376 † | 2.635 | 7.01× | 265887 | 73.3 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.261 | 7.710 | 1.47× | 19008 | 66.8 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1074128 (cap 1048576)

