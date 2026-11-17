# searchdump sequential speed suite — os13-secure (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE; × faster median 1.75× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T22:51:31-05:00, finished 2026-11-16T23:49:33-05:00
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

## searchdump × os13-secure (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.371 | 4.158 | 1.75× | 42170 | 73.8 | — | PASS (NO_BASELINE) |
| restore-none | 18.090 | 20.865 | 1.15× | 5528 | 82.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.479 | 13.005 | 5.25× | 40341 | 100.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 18.140 | 21.508 | 1.19× | 5513 | 102.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.268 | — | — | 44083 | 93.0 | — | PASS (NO_BASELINE) |
| restore-zstd | 18.146 | — | — | 5511 | 99.8 | — | PASS (NO_BASELINE) |
| copy | 17.848 | 20.783 | 1.16× | 5603 | 127.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.284 | — | — | 43787 | 76.1 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 2.304 | — | — | 43398 | 100.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.260 | — | — | 79350 | 166.8 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 0.760 | — | — | 131587 | 224.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 2.337 | 4.170 | 1.78× | 42788 | 76.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 18.346 | 24.794 | 1.35× | 5451 | 98.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.432 | 5.358 | 2.20× | 41116 | 122.6 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 18.305 | 24.584 | 1.34× | 5463 | 98.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.377 | — | — | 42066 | 105.9 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 18.088 | — | — | 5529 | 101.1 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | 0.709 | 26.26× | 37 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.086 † | 0.853 | 9.92× | 12 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.027 † | 0.684 | 25.33× | 37 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.223 † | 0.917 | 4.11× | 4 | 26.8 | — | PASS (NO_BASELINE) |
| restore-norefresh | 12.256 | 14.457 | 1.18× | 8159 | 100.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.409 | 14.623 | 1.18× | 8059 | 129.6 | — | PASS (NO_BASELINE) |
| dump-small | 1.193 | 2.470 | 2.07× | 83835 | 60.6 | — | PASS (NO_BASELINE) |
| restore-small | 10.769 | 13.008 | 1.21× | 9286 | 74.6 | — | PASS (NO_BASELINE) |
| dump-big | 1.948 | 4.535 | 2.33× | 17 | 263.9 | — | PASS (NO_BASELINE) |
| restore-big | 7.199 | 10.096 | 1.40× | 5 | 398.5 | — | PASS (NO_BASELINE) |
| dump-edge | 4.147 | — | — | 2274 | 304.4 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 6.690 | — | — | 1410 | 622.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.966 | 16.306 | 5.50× | 33720 | 78.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 28.401 | 44.044 | 1.55× | 3521 | 77.1 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.815 | 4.818 | 5.91× | 122767 | 78.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.311 | 13.744 | 1.48× | 10740 | 78.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.455 † | 2.927 | 6.43× | 219996 | 74.4 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.414 | 8.767 | 1.37× | 15590 | 76.4 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1215584 (cap 1048576)

