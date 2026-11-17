# searchdump sequential speed suite — os2-secure (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T03:54:47-05:00, finished 2026-11-17T04:23:52-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os2-secure (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.310 | — | — | 43287 | 69.2 | — | PASS (NO_BASELINE) |
| restore-none | 12.527 | — | — | 7983 | 108.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.442 | — | — | 40956 | 85.2 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.017 | — | — | 7682 | 98.1 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.219 | — | — | 45065 | 84.5 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.446 | — | — | 8035 | 102.1 | — | PASS (NO_BASELINE) |
| copy | 13.444 | — | — | 7438 | 143.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.933 | — | — | 51733 | 72.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 3.059 | — | — | 32693 | 69.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.478 | — | — | 40351 | 103.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.879 | — | — | 34729 | 102.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.469 | — | — | 68081 | 134.8 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.514 | — | — | 66043 | 169.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.797 | — | — | 125482 | 196.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 1.026 | — | — | 97457 | 221.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.200 | — | — | 45458 | 67.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.962 | — | — | 7715 | 97.5 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.218 | — | — | 45085 | 88.5 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.789 | — | — | 7819 | 110.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.355 | — | — | 42472 | 83.4 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.644 | — | — | 7909 | 99.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.028 † | — | — | 36 | 26.6 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.113 † | — | — | 9 | 26.8 | — | PASS (NO_BASELINE) |
| settings-dump | 0.030 † | — | — | 33 | 26.8 | — | PASS (NO_BASELINE) |
| settings-restore | 0.261 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 12.137 | — | — | 8239 | 101.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.970 | — | — | 8354 | 134.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.166 | — | — | 85766 | 45.0 | — | PASS (NO_BASELINE) |
| restore-small | 6.703 | — | — | 14918 | 88.5 | — | PASS (NO_BASELINE) |
| dump-big | 2.036 | — | — | 16 | 252.9 | — | PASS (NO_BASELINE) |
| restore-big | 3.608 | — | — | 9 | 417.2 | — | PASS (NO_BASELINE) |
| dump-edge | 3.567 | — | — | 2644 | 309.5 | — | PASS (NO_BASELINE) |
| restore-edge | 3.241 | — | — | 2910 | 506.2 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.744 | — | — | 36442 | 74.6 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 28.938 | — | — | 3456 | 79.2 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.785 | — | — | 127404 | 78.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.730 | — | — | 12936 | 83.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.407 † | — | — | 245495 | 76.8 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.349 | — | — | 18694 | 83.5 | — | PASS (NO_BASELINE) |

