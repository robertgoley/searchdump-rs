# searchdump sequential speed suite — es819-plain (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T15:08:48-05:00, finished 2026-11-16T15:32:02-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es819-plain (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.897 | — | — | 52711 | 101.5 | — | PASS (NO_BASELINE) |
| restore-none | 10.209 | — | — | 9795 | 109.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.860 | — | — | 53774 | 99.7 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.170 | — | — | 9833 | 114.5 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.904 | — | — | 52529 | 104.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.309 | — | — | 9700 | 110.4 | — | PASS (NO_BASELINE) |
| copy | 10.646 | — | — | 9393 | 163.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.851 | — | — | 54036 | 91.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.129 | — | — | 46965 | 88.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.873 | — | — | 53399 | 110.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.166 | — | — | 85748 | 146.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 0.981 | — | — | 101897 | 146.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.647 | — | — | 154666 | 198.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.586 | — | — | 170704 | 212.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.381 † | — | — | 262536 | 268.3 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.804 | — | — | 55442 | 100.1 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.178 | — | — | 9825 | 99.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.889 | — | — | 52940 | 110.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.291 | — | — | 9718 | 99.2 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.900 | — | — | 52624 | 120.2 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.092 | — | — | 9909 | 104.1 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 38 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.075 † | — | — | 13 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.256 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.521 | — | — | 10503 | 111.6 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.323 | — | — | 9688 | 153.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.015 | — | — | 98516 | 67.9 | — | PASS (NO_BASELINE) |
| restore-small | 6.456 | — | — | 15491 | 88.4 | — | PASS (NO_BASELINE) |
| dump-big | 0.873 | — | — | 38 | 308.3 | — | PASS (NO_BASELINE) |
| restore-big | 3.328 | — | — | 10 | 449.2 | — | PASS (NO_BASELINE) |
| dump-edge | 1.960 | — | — | 4811 | 309.9 | — | PASS (NO_BASELINE) |
| restore-edge | 2.501 | — | — | 3771 | 593.5 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.075 | — | — | 48197 | 70.0 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.485 | — | — | 3924 | 77.1 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.553 | — | — | 180806 | 71.2 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.700 | — | — | 11494 | 78.3 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.313 † | — | — | 319241 | 73.1 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.161 | — | — | 13964 | 76.8 | — | PASS (NO_BASELINE) |

