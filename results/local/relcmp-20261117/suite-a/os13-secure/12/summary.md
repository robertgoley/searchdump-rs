# searchdump sequential speed suite — os13-secure (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T00:18:18-05:00, finished 2026-11-17T00:46:23-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os13-secure (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.115 | — | — | 47281 | 75.0 | — | PASS (NO_BASELINE) |
| restore-none | 12.472 | — | — | 8018 | 112.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.025 | — | — | 49388 | 78.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 12.581 | — | — | 7949 | 96.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.045 | — | — | 48892 | 87.7 | — | PASS (NO_BASELINE) |
| restore-zstd | 11.908 | — | — | 8398 | 104.7 | — | PASS (NO_BASELINE) |
| copy | 12.850 | — | — | 7782 | 134.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.083 | — | — | 48000 | 78.9 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 2.863 | — | — | 34927 | 108.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.677 | — | — | 59618 | 124.7 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 1.253 | — | — | 79796 | 187.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 2.080 | — | — | 48078 | 74.9 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.121 | — | — | 8250 | 102.6 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.004 | — | — | 49910 | 94.7 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.343 | — | — | 8102 | 99.5 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.035 | — | — | 49147 | 97.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.055 | — | — | 8295 | 101.2 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 36 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.095 † | — | — | 10 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.027 † | — | — | 36 | 27.3 | — | PASS (NO_BASELINE) |
| settings-restore | 0.245 † | — | — | 4 | 26.7 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.056 | — | — | 9045 | 106.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.224 | — | — | 8909 | 129.8 | — | PASS (NO_BASELINE) |
| dump-small | 1.151 | — | — | 86854 | 48.6 | — | PASS (NO_BASELINE) |
| restore-small | 6.671 | — | — | 14990 | 79.0 | — | PASS (NO_BASELINE) |
| dump-big | 1.928 | — | — | 17 | 281.7 | — | PASS (NO_BASELINE) |
| restore-big | 5.695 | — | — | 6 | 434.7 | — | PASS (NO_BASELINE) |
| dump-edge | 3.925 | — | — | 2403 | 260.4 | — | PASS (NO_BASELINE) |
| restore-edge | 5.323 | — | — | 1772 | 581.5 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 3.023 | — | — | 33076 | 78.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 24.443 | — | — | 4091 | 79.1 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.808 | — | — | 123690 | 76.2 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.234 | — | — | 12145 | 77.6 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.447 † | — | — | 223482 | 76.5 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.887 | — | — | 16986 | 75.2 | — | PASS (NO_BASELINE) |

