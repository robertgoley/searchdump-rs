# searchdump sequential speed suite — es710-plain (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE; × faster median 2.64× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T03:18:45-05:00, finished 2026-11-16T04:11:49-05:00
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

## searchdump × es710-plain (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.273 | 3.426 | 2.69× | 78562 | 103.4 | — | PASS (NO_BASELINE) |
| restore-none | 12.927 | 15.130 | 1.17× | 7736 | 83.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.223 | 13.014 | 10.64× | 81748 | 125.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 12.563 | 15.057 | 1.20× | 7960 | 99.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.151 | — | — | 86881 | 123.1 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.497 | — | — | 8002 | 93.4 | — | PASS (NO_BASELINE) |
| copy | 12.557 | 15.189 | 1.21× | 7964 | 155.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.273 | — | — | 78565 | 107.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 4.224 | — | — | 23675 | 105.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 2.198 | — | — | 45486 | 140.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 1.299 | — | — | 77005 | 184.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.352 | 3.576 | 2.64× | 73977 | 93.3 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.722 | 18.601 | 1.46× | 7860 | 113.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.396 | 4.832 | 3.46× | 71609 | 123.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.629 | 18.647 | 1.48× | 7918 | 97.8 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.444 | — | — | 69233 | 137.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.707 | — | — | 7870 | 105.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.665 | 27.71× | 41 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.063 † | 0.768 | 12.19× | 16 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.661 | 27.54× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.189 † | 0.851 | 4.50× | 5 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.699 | 11.579 | 1.19× | 10311 | 101.3 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.573 | 11.848 | 1.24× | 10446 | 142.4 | — | PASS (NO_BASELINE) |
| dump-small | 0.753 | 2.115 | 2.81× | 132772 | 73.8 | — | PASS (NO_BASELINE) |
| restore-small | 7.358 | 9.393 | 1.28× | 13591 | 71.2 | — | PASS (NO_BASELINE) |
| dump-big | 1.241 | 3.782 | 3.05× | 27 | 285.8 | — | PASS (NO_BASELINE) |
| restore-big | 5.698 | 8.201 | 1.44× | 6 | 437.6 | — | PASS (NO_BASELINE) |
| dump-edge | 3.492 | — | — | 2701 | 296.0 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.907 | — | — | 1922 | 608.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.139 | 14.774 | 6.91× | 46746 | 74.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 23.485 | 38.617 | 1.64× | 4258 | 73.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.550 | 4.414 | 8.03× | 181712 | 73.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.615 | 12.147 | 1.60× | 13132 | 67.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.328 † | 2.669 | 8.14× | 305204 | 73.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.272 | 7.773 | 1.47× | 18968 | 67.1 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1739044 (cap 1048576)

