# searchdump sequential speed suite — es717-plain (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T08:42:58-05:00, finished 2026-11-16T09:06:52-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es717-plain (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.884 | — | — | 53087 | 98.2 | — | PASS (NO_BASELINE) |
| restore-none | 10.065 | — | — | 9936 | 112.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.837 | — | — | 54438 | 108.0 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.089 | — | — | 9912 | 108.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.784 | — | — | 56059 | 114.9 | — | PASS (NO_BASELINE) |
| restore-zstd | 9.841 | — | — | 10161 | 112.1 | — | PASS (NO_BASELINE) |
| copy | 10.452 | — | — | 9567 | 153.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.842 | — | — | 54281 | 90.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.068 | — | — | 48362 | 102.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.804 | — | — | 55432 | 131.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.125 | — | — | 88865 | 149.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.004 | — | — | 99554 | 162.9 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.631 | — | — | 158589 | 206.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.651 | — | — | 153626 | 238.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.388 † | — | — | 257968 | 291.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.919 | — | — | 52120 | 99.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.092 | — | — | 9016 | 99.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.765 | — | — | 56661 | 116.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.226 | — | — | 8908 | 100.4 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.857 | — | — | 53836 | 110.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.252 | — | — | 8887 | 100.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | — | — | 41 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.075 † | — | — | 13 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 39 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.192 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.611 | — | — | 10405 | 103.4 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.292 | — | — | 9716 | 143.8 | — | PASS (NO_BASELINE) |
| dump-small | 0.931 | — | — | 107410 | 70.0 | — | PASS (NO_BASELINE) |
| restore-small | 6.181 | — | — | 16178 | 94.0 | — | PASS (NO_BASELINE) |
| dump-big | 1.001 | — | — | 33 | 314.8 | — | PASS (NO_BASELINE) |
| restore-big | 5.307 | — | — | 6 | 432.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.374 | — | — | 3972 | 326.0 | — | PASS (NO_BASELINE) |
| restore-edge | 4.416 | — | — | 2136 | 599.8 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.172 | — | — | 46048 | 73.9 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.321 | — | — | 3949 | 72.5 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.584 | — | — | 171218 | 71.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.971 | — | — | 10030 | 75.7 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.324 † | — | — | 309006 | 70.4 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.774 | — | — | 14763 | 74.8 | — | PASS (NO_BASELINE) |

