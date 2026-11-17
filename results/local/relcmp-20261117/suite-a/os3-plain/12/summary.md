# searchdump sequential speed suite — os3-plain (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T05:41:52-05:00, finished 2026-11-17T06:10:05-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os3-plain (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.824 | — | — | 54828 | 97.9 | — | PASS (NO_BASELINE) |
| restore-none | 12.607 | — | — | 7932 | 98.4 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.643 | — | — | 60850 | 100.4 | — | PASS (NO_BASELINE) |
| restore-gzip | 12.405 | — | — | 8061 | 102.2 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.506 | — | — | 66410 | 100.2 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.261 | — | — | 8156 | 104.9 | — | PASS (NO_BASELINE) |
| copy | 12.926 | — | — | 7736 | 146.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.628 | — | — | 61412 | 83.5 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.955 | — | — | 51154 | 96.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.177 | — | — | 45939 | 117.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.538 | — | — | 39409 | 133.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.138 | — | — | 87880 | 151.9 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.395 | — | — | 71687 | 174.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.711 | — | — | 140701 | 232.4 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.887 | — | — | 112702 | 246.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.601 | — | — | 62475 | 107.5 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.254 | — | — | 8160 | 101.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.593 | — | — | 62765 | 118.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.134 | — | — | 8241 | 102.1 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.749 | — | — | 57176 | 133.1 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 13.639 | — | — | 7332 | 100.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.059 † | — | — | 17 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.278 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.055 † | — | — | 18 | 27.0 | — | PASS (NO_BASELINE) |
| settings-restore | 0.874 | — | — | 1 | 26.9 | — | PASS (NO_BASELINE) |
| restore-norefresh | 11.171 | — | — | 8952 | 103.8 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.169 | — | — | 8218 | 153.3 | — | PASS (NO_BASELINE) |
| dump-small | 0.977 | — | — | 102366 | 72.7 | — | PASS (NO_BASELINE) |
| restore-small | 6.380 | — | — | 15674 | 85.5 | — | PASS (NO_BASELINE) |
| dump-big | 1.080 | — | — | 30 | 250.0 | — | PASS (NO_BASELINE) |
| restore-big | 3.674 | — | — | 9 | 446.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.407 | — | — | 3919 | 331.7 | — | PASS (NO_BASELINE) |
| restore-edge | 2.865 | — | — | 3292 | 502.4 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 3.011 | — | — | 33213 | 73.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 28.486 | — | — | 3510 | 77.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.662 | — | — | 151154 | 74.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.764 | — | — | 12880 | 75.8 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 1.369 | — | — | 73073 | 73.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 16.989 | — | — | 5886 | 75.5 | — | PASS (NO_BASELINE) |

