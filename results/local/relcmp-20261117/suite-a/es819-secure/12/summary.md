# searchdump sequential speed suite — es819-secure (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T17:19:41-05:00, finished 2026-11-16T17:47:02-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es819-secure (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.332 | — | — | 42883 | 73.1 | — | PASS (NO_BASELINE) |
| restore-none | 10.641 | — | — | 9398 | 110.6 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.291 | — | — | 43640 | 91.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.641 | — | — | 9398 | 106.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.232 | — | — | 44806 | 80.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.499 | — | — | 9525 | 106.0 | — | PASS (NO_BASELINE) |
| copy | 11.163 | — | — | 8958 | 139.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.301 | — | — | 43458 | 73.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.582 | — | — | 38732 | 77.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.058 | — | — | 48590 | 110.6 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.349 | — | — | 74147 | 109.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.146 | — | — | 87230 | 147.8 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.753 | — | — | 132809 | 169.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.666 | — | — | 150071 | 223.5 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.468 † | — | — | 213841 | 249.4 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.296 | — | — | 43556 | 78.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.744 | — | — | 9308 | 103.3 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.156 | — | — | 46372 | 117.0 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.757 | — | — | 9296 | 107.0 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.249 | — | — | 44474 | 93.9 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.509 | — | — | 9515 | 107.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 36 | 27.3 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.121 † | — | — | 8 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.394 † | — | — | 2 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.084 | — | — | 9916 | 106.6 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.786 | — | — | 9271 | 143.4 | — | PASS (NO_BASELINE) |
| dump-small | 1.146 | — | — | 87227 | 65.0 | — | PASS (NO_BASELINE) |
| restore-small | 6.748 | — | — | 14820 | 86.4 | — | PASS (NO_BASELINE) |
| dump-big | 1.153 | — | — | 29 | 263.3 | — | PASS (NO_BASELINE) |
| restore-big | 3.569 | — | — | 9 | 469.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.676 | — | — | 3525 | 314.7 | — | PASS (NO_BASELINE) |
| restore-edge | 3.058 | — | — | 3084 | 592.9 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.322 | — | — | 43066 | 76.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 27.777 | — | — | 3600 | 81.9 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.605 | — | — | 165324 | 79.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.191 | — | — | 9812 | 78.0 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.381 † | — | — | 262657 | 73.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.869 | — | — | 12709 | 79.8 | — | PASS (NO_BASELINE) |

