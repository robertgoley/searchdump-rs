# searchdump sequential speed suite — es710-plain (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T04:11:50-05:00, finished 2026-11-16T04:35:57-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es710-plain (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.335 | — | — | 74881 | 85.6 | — | PASS (NO_BASELINE) |
| restore-none | 9.104 | — | — | 10984 | 111.0 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.346 | — | — | 74314 | 113.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.018 | — | — | 11089 | 111.5 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.253 | — | — | 79804 | 101.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 8.785 | — | — | 11384 | 111.8 | — | PASS (NO_BASELINE) |
| copy | 9.201 | — | — | 10868 | 151.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.299 | — | — | 77009 | 95.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.391 | — | — | 71908 | 87.3 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.290 | — | — | 77514 | 91.7 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 1.250 | — | — | 79999 | 83.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.345 | — | — | 74340 | 98.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 9.741 | — | — | 10266 | 99.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.289 | — | — | 77605 | 119.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 9.752 | — | — | 10254 | 99.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.256 | — | — | 79627 | 134.7 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 9.655 | — | — | 10357 | 98.3 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 39 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.069 † | — | — | 14 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.223 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 8.440 | — | — | 11849 | 106.7 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.019 | — | — | 11088 | 149.5 | — | PASS (NO_BASELINE) |
| dump-small | 0.739 | — | — | 135366 | 79.2 | — | PASS (NO_BASELINE) |
| restore-small | 5.934 | — | — | 16852 | 87.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.061 | — | — | 31 | 295.3 | — | PASS (NO_BASELINE) |
| restore-big | 4.830 | — | — | 7 | 435.1 | — | PASS (NO_BASELINE) |
| dump-edge | 3.346 | — | — | 2818 | 308.3 | — | PASS (NO_BASELINE) |
| restore-edge | 3.192 | — | — | 2955 | 589.4 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.090 | — | — | 47838 | 75.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.225 | — | — | 4712 | 75.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.580 | — | — | 172480 | 71.9 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.132 | — | — | 14022 | 73.3 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.338 † | — | — | 295722 | 73.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 4.956 | — | — | 20176 | 74.7 | — | PASS (NO_BASELINE) |

