# searchdump sequential speed suite — os3-plain (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE; × faster median 1.97× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T04:23:53-05:00, finished 2026-11-17T05:16:35-05:00
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

## searchdump × os3-plain (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.835 | 3.620 | 1.97× | 54509 | 93.1 | — | PASS (NO_BASELINE) |
| restore-none | 17.157 | 19.451 | 1.13× | 5829 | 99.6 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.601 | 13.199 | 8.24× | 62452 | 125.0 | — | PASS (NO_BASELINE) |
| restore-gzip | 16.651 | 19.317 | 1.16× | 6006 | 97.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.764 | — | — | 56674 | 118.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 18.812 | — | — | 5316 | 93.9 | — | PASS (NO_BASELINE) |
| copy | 17.505 | 19.735 | 1.13× | 5713 | 143.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.727 | — | — | 57890 | 91.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.849 | — | — | 54073 | 106.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.092 | — | — | 47803 | 131.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.584 | — | — | 38703 | 137.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.235 | — | — | 80943 | 153.7 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.393 | — | — | 71784 | 178.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.658 | — | — | 151974 | 223.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.898 | — | — | 111346 | 256.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.703 | 3.623 | 2.13× | 58716 | 109.1 | — | PASS (NO_BASELINE) |
| restore-split-none | 17.784 | 22.790 | 1.28× | 5623 | 100.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.664 | 5.016 | 3.01× | 60104 | 129.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 16.906 | 23.709 | 1.40× | 5915 | 95.8 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.786 | — | — | 56006 | 133.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 17.285 | — | — | 5785 | 99.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.680 | 28.33× | 41 | 27.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.080 † | 0.803 | 10.04× | 13 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.023 † | 0.657 | 28.57× | 43 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.216 † | 0.872 | 4.04× | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 12.439 | 14.650 | 1.18× | 8039 | 97.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.203 | 14.114 | 1.16× | 8194 | 151.6 | — | PASS (NO_BASELINE) |
| dump-small | 0.915 | 2.152 | 2.35× | 109330 | 67.7 | — | PASS (NO_BASELINE) |
| restore-small | 9.821 | 12.092 | 1.23× | 10182 | 66.4 | — | PASS (NO_BASELINE) |
| dump-big | 1.260 | 3.793 | 3.01× | 26 | 321.8 | — | PASS (NO_BASELINE) |
| restore-big | 4.116 | 6.527 | 1.59× | 8 | 438.0 | — | PASS (NO_BASELINE) |
| dump-edge | 2.369 | — | — | 3981 | 349.7 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 4.441 | — | — | 2124 | 607.6 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.099 | 14.992 | 7.14× | 47641 | 70.2 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 24.935 | 48.423 | 1.94× | 4010 | 75.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.584 | 4.425 | 7.58× | 171366 | 75.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.971 | 12.491 | 1.57× | 12545 | 68.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.329 † | 2.669 | 8.11× | 303966 | 72.8 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.292 | 7.841 | 1.48× | 18896 | 68.2 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1911176 (cap 1048576)

