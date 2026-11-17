# searchdump sequential speed suite — os13-plain (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE; × faster median 2.01× over 25 Node-compared cases

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T21:18:08-05:00, finished 2026-11-16T22:06:31-05:00
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

## searchdump × os13-plain (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.956 | 3.934 | 2.01× | 51117 | 91.9 | — | PASS (NO_BASELINE) |
| restore-none | 14.063 | 16.433 | 1.17× | 7111 | 89.1 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.851 | 13.147 | 7.10× | 54033 | 117.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 14.180 | 16.803 | 1.18× | 7052 | 92.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.782 | — | — | 56115 | 126.9 | — | PASS (NO_BASELINE) |
| restore-zstd | 14.107 | — | — | 7089 | 96.7 | — | PASS (NO_BASELINE) |
| copy | 14.035 | 16.587 | 1.18× | 7125 | 153.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.730 | — | — | 57810 | 97.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.961 | — | — | 51006 | 114.6 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.017 | — | — | 98361 | 168.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 0.632 | — | — | 158287 | 224.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.858 | 3.758 | 2.02× | 53835 | 98.6 | — | PASS (NO_BASELINE) |
| restore-split-none | 14.112 | 20.315 | 1.44× | 7086 | 103.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.845 | 5.011 | 2.72× | 54194 | 124.2 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 14.235 | 19.935 | 1.40× | 7025 | 99.4 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.793 | — | — | 55762 | 128.3 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 14.239 | — | — | 7023 | 97.6 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | 0.675 | 28.12× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.063 † | 0.773 | 12.27× | 16 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | 0.672 | 28.00× | 42 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.180 † | 0.854 | 4.74× | 6 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.070 | 13.074 | 1.18× | 9033 | 99.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.222 | 13.468 | 1.20× | 8911 | 141.8 | — | PASS (NO_BASELINE) |
| dump-small | 1.037 | 2.329 | 2.25× | 96468 | 79.6 | — | PASS (NO_BASELINE) |
| restore-small | 8.040 | 10.208 | 1.27× | 12438 | 77.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.559 | 4.065 | 2.61× | 21 | 256.9 | — | PASS (NO_BASELINE) |
| restore-big | 6.221 | 8.488 | 1.36× | 5 | 441.7 | — | PASS (NO_BASELINE) |
| dump-edge | 3.333 | — | — | 2830 | 315.2 | — | PASS (NO_BASELINE); NODE_DIFF |
| restore-edge | 5.734 | — | — | 1645 | 611.3 | — | PASS (NO_BASELINE); NODE_FAIL |
| multi-dump-p1 | 2.778 | 15.597 | 5.61× | 35991 | 73.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.638 | 41.421 | 1.62× | 3900 | 73.0 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.710 | 4.435 | 6.25× | 140922 | 71.9 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.029 | 12.381 | 1.54× | 12455 | 68.7 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.391 † | 2.660 | 6.80× | 255880 | 69.5 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.343 | 7.949 | 1.49× | 18717 | 69.9 | — | PASS (NO_BASELINE) |

Node (informational, not an rs failure): dump-edge/node/rep-1: NODE_DIFF: edverify diff dump-edge-node-warmup: document sets differ (report verify-dump-edge-node-warmup.json)

Node (informational, not an rs failure): restore-edge/node/rep-1: NODE_FAIL: metrics/write/RSS gate: schema errors=[] failed_writes=None max_rss_kb=1910440 (cap 1048576)

