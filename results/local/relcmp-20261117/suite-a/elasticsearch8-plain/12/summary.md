# searchdump sequential speed suite — elasticsearch8-plain (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T12:10:11-05:00, finished 2026-11-16T12:33:12-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch8-plain (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.878 | — | — | 53248 | 84.0 | — | PASS (NO_BASELINE) |
| restore-none | 9.037 | — | — | 11065 | 95.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.816 | — | — | 55057 | 102.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.246 | — | — | 10815 | 105.1 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.924 | — | — | 51978 | 102.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 9.012 | — | — | 11096 | 105.0 | — | PASS (NO_BASELINE) |
| copy | 9.450 | — | — | 10582 | 147.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.900 | — | — | 52641 | 84.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.225 | — | — | 44945 | 88.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.889 | — | — | 52945 | 124.0 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.170 | — | — | 85474 | 136.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.070 | — | — | 93479 | 160.5 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.614 | — | — | 162796 | 188.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.602 | — | — | 166218 | 236.7 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.392 † | — | — | 254820 | 260.7 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.873 | — | — | 53401 | 102.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 9.728 | — | — | 10279 | 98.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.870 | — | — | 53462 | 123.6 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 9.821 | — | — | 10182 | 103.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.824 | — | — | 54826 | 104.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 9.600 | — | — | 10416 | 99.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.076 † | — | — | 13 | 27.0 | — | PASS (NO_BASELINE) |
| settings-dump | 0.028 † | — | — | 35 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.240 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 8.657 | — | — | 11551 | 98.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.131 | — | — | 10952 | 144.4 | — | PASS (NO_BASELINE) |
| dump-small | 0.921 | — | — | 108524 | 66.2 | — | PASS (NO_BASELINE) |
| restore-small | 5.926 | — | — | 16874 | 77.8 | — | PASS (NO_BASELINE) |
| dump-big | 0.941 | — | — | 35 | 313.9 | — | PASS (NO_BASELINE) |
| restore-big | 3.547 | — | — | 9 | 454.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.491 | — | — | 3786 | 310.8 | — | PASS (NO_BASELINE) |
| restore-edge | 2.994 | — | — | 3150 | 671.7 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.547 | — | — | 39263 | 73.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.770 | — | — | 3736 | 73.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.593 | — | — | 168556 | 75.2 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.220 | — | — | 10846 | 75.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.339 † | — | — | 294880 | 71.5 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.326 | — | — | 13651 | 79.3 | — | PASS (NO_BASELINE) |

