# searchdump sequential speed suite — os13-secure (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T23:49:34-05:00, finished 2026-11-17T00:18:17-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os13-secure (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.053 | — | — | 48705 | 74.2 | — | PASS (NO_BASELINE) |
| restore-none | 13.202 | — | — | 7575 | 102.0 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.036 | — | — | 49105 | 84.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.208 | — | — | 7571 | 98.9 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.742 | — | — | 57414 | 83.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.584 | — | — | 7947 | 106.0 | — | PASS (NO_BASELINE) |
| copy | 13.025 | — | — | 7677 | 131.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.853 | — | — | 53958 | 78.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 2.855 | — | — | 35020 | 114.6 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.657 | — | — | 60360 | 153.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 1.641 | — | — | 60954 | 207.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.990 | — | — | 50247 | 70.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.950 | — | — | 7722 | 97.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.092 | — | — | 47796 | 102.5 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 13.137 | — | — | 7612 | 102.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.109 | — | — | 47423 | 104.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.880 | — | — | 7764 | 103.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.088 † | — | — | 11 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.230 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 12.107 | — | — | 8259 | 99.4 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.219 | — | — | 8184 | 128.5 | — | PASS (NO_BASELINE) |
| dump-small | 1.147 | — | — | 87173 | 54.8 | — | PASS (NO_BASELINE) |
| restore-small | 6.894 | — | — | 14505 | 88.9 | — | PASS (NO_BASELINE) |
| dump-big | 1.924 | — | — | 17 | 298.5 | — | PASS (NO_BASELINE) |
| restore-big | 5.734 | — | — | 6 | 440.6 | — | PASS (NO_BASELINE) |
| dump-edge | 4.459 | — | — | 2115 | 300.3 | — | PASS (NO_BASELINE) |
| restore-edge | 5.093 | — | — | 1852 | 505.5 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 3.562 | — | — | 28076 | 78.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 24.699 | — | — | 4049 | 80.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.918 | — | — | 108899 | 76.5 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.590 | — | — | 11641 | 75.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.498 † | — | — | 200923 | 71.9 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.784 | — | — | 17290 | 75.8 | — | PASS (NO_BASELINE) |

