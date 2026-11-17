# searchdump sequential speed suite — es819-plain (8.19.23)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T15:32:03-05:00, finished 2026-11-16T15:55:15-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es819-plain (8.19.23)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.017 | — | — | 49582 | 95.0 | — | PASS (NO_BASELINE) |
| restore-none | 10.033 | — | — | 9967 | 113.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.908 | — | — | 52410 | 109.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.796 | — | — | 10208 | 107.3 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.875 | — | — | 53319 | 100.4 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.000 | — | — | 10000 | 113.7 | — | PASS (NO_BASELINE) |
| copy | 10.336 | — | — | 9675 | 157.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.872 | — | — | 53425 | 92.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.183 | — | — | 45808 | 97.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.888 | — | — | 52965 | 110.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.166 | — | — | 85784 | 152.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.035 | — | — | 96611 | 140.4 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.642 | — | — | 155732 | 200.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.594 | — | — | 168280 | 208.7 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.384 † | — | — | 260112 | 268.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.925 | — | — | 51941 | 104.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.289 | — | — | 9720 | 98.2 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.923 | — | — | 52009 | 119.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.304 | — | — | 9705 | 100.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.940 | — | — | 51543 | 129.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.338 | — | — | 9673 | 106.6 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.072 † | — | — | 14 | 27.0 | — | PASS (NO_BASELINE) |
| settings-dump | 0.024 † | — | — | 42 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.250 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.725 | — | — | 10282 | 115.3 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.184 | — | — | 9820 | 158.7 | — | PASS (NO_BASELINE) |
| dump-small | 1.060 | — | — | 94364 | 65.9 | — | PASS (NO_BASELINE) |
| restore-small | 6.554 | — | — | 15257 | 89.9 | — | PASS (NO_BASELINE) |
| dump-big | 0.925 | — | — | 36 | 300.6 | — | PASS (NO_BASELINE) |
| restore-big | 3.511 | — | — | 9 | 455.8 | — | PASS (NO_BASELINE) |
| dump-edge | 2.081 | — | — | 4532 | 344.3 | — | PASS (NO_BASELINE) |
| restore-edge | 2.651 | — | — | 3558 | 580.0 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.007 | — | — | 49823 | 74.5 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.424 | — | — | 3933 | 75.7 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.552 | — | — | 181305 | 72.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 8.860 | — | — | 11287 | 73.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.326 † | — | — | 307092 | 71.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.167 | — | — | 13953 | 76.3 | — | PASS (NO_BASELINE) |

