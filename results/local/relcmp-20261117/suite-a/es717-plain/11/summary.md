# searchdump sequential speed suite — es717-plain (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T08:17:40-05:00, finished 2026-11-16T08:42:57-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es717-plain (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.903 | — | — | 52554 | 103.3 | — | PASS (NO_BASELINE) |
| restore-none | 10.052 | — | — | 9948 | 107.3 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.697 | — | — | 58934 | 111.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.963 | — | — | 10037 | 120.8 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.918 | — | — | 52135 | 101.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.240 | — | — | 9766 | 105.1 | — | PASS (NO_BASELINE) |
| copy | 10.526 | — | — | 9500 | 154.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.929 | — | — | 51837 | 89.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.056 | — | — | 48643 | 85.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 3.174 | — | — | 31504 | 107.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.146 | — | — | 87244 | 158.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.656 | — | — | 60392 | 138.5 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.645 | — | — | 155026 | 195.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.954 | — | — | 104795 | 210.5 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.412 † | — | — | 242428 | 274.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.914 | — | — | 52244 | 99.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.088 | — | — | 8273 | 95.9 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.864 | — | — | 53657 | 119.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.136 | — | — | 8240 | 101.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.921 | — | — | 52068 | 115.4 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.901 | — | — | 8402 | 103.9 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.083 † | — | — | 12 | 26.9 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 39 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.204 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.667 | — | — | 10344 | 109.5 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.456 | — | — | 9563 | 140.3 | — | PASS (NO_BASELINE) |
| dump-small | 1.026 | — | — | 97433 | 71.1 | — | PASS (NO_BASELINE) |
| restore-small | 6.529 | — | — | 15316 | 83.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.136 | — | — | 29 | 299.0 | — | PASS (NO_BASELINE) |
| restore-big | 5.350 | — | — | 6 | 433.6 | — | PASS (NO_BASELINE) |
| dump-edge | 2.351 | — | — | 4011 | 328.3 | — | PASS (NO_BASELINE) |
| restore-edge | 3.999 | — | — | 2358 | 513.4 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.139 | — | — | 46746 | 74.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.511 | — | — | 3920 | 77.2 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.566 | — | — | 176798 | 69.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 9.973 | — | — | 10027 | 78.5 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.309 † | — | — | 323339 | 77.4 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.889 | — | — | 14516 | 78.6 | — | PASS (NO_BASELINE) |

