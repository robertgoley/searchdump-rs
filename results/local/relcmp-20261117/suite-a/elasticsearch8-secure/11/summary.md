# searchdump sequential speed suite — elasticsearch8-secure (8.6.1)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T13:25:43-05:00, finished 2026-11-16T13:51:23-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch8-secure (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.282 | — | — | 43829 | 72.0 | — | PASS (NO_BASELINE) |
| restore-none | 9.173 | — | — | 10902 | 109.0 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.156 | — | — | 46381 | 83.2 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.263 | — | — | 10796 | 104.4 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.091 | — | — | 47830 | 95.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 9.230 | — | — | 10834 | 108.8 | — | PASS (NO_BASELINE) |
| copy | 10.002 | — | — | 9998 | 134.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.211 | — | — | 45225 | 76.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.548 | — | — | 39244 | 66.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.046 | — | — | 48869 | 107.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.308 | — | — | 76449 | 110.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.078 | — | — | 92787 | 169.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.721 | — | — | 138695 | 171.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.653 | — | — | 153181 | 226.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.425 † | — | — | 235206 | 243.0 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.216 | — | — | 45134 | 76.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 9.947 | — | — | 10053 | 102.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.167 | — | — | 46149 | 93.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 9.842 | — | — | 10160 | 104.4 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.207 | — | — | 45307 | 95.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 9.897 | — | — | 10104 | 109.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 40 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.071 † | — | — | 14 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | — | — | 41 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.234 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.034 | — | — | 11069 | 111.1 | — | PASS (NO_BASELINE) |
| copy-norefresh | 9.664 | — | — | 10348 | 130.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.084 | — | — | 92270 | 58.6 | — | PASS (NO_BASELINE) |
| restore-small | 5.915 | — | — | 16906 | 88.6 | — | PASS (NO_BASELINE) |
| dump-big | 1.125 | — | — | 29 | 308.4 | — | PASS (NO_BASELINE) |
| restore-big | 3.469 | — | — | 10 | 451.8 | — | PASS (NO_BASELINE) |
| dump-edge | 2.811 | — | — | 3355 | 328.1 | — | PASS (NO_BASELINE) |
| restore-edge | 3.086 | — | — | 3056 | 516.6 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.499 | — | — | 40022 | 73.0 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 26.483 | — | — | 3776 | 75.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.632 | — | — | 158176 | 73.0 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.282 | — | — | 10773 | 77.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.391 † | — | — | 256026 | 78.5 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.173 | — | — | 13942 | 77.1 | — | PASS (NO_BASELINE) |

