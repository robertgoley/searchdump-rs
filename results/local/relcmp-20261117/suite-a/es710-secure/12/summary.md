# searchdump sequential speed suite — es710-secure (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T06:48:14-05:00, finished 2026-11-16T07:25:54-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es710-secure (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.106 | — | — | 47478 | 76.6 | — | PASS (NO_BASELINE) |
| restore-none | 10.664 | — | — | 9377 | 100.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.137 | — | — | 46792 | 85.0 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.547 | — | — | 9481 | 117.9 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.995 | — | — | 50116 | 84.2 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.612 | — | — | 9423 | 106.9 | — | PASS (NO_BASELINE) |
| copy | 11.091 | — | — | 9016 | 154.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.153 | — | — | 46442 | 73.3 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.785 | — | — | 56020 | 68.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 2.021 | — | — | 49480 | 63.1 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 2.211 | — | — | 45231 | 68.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 2.143 | — | — | 46664 | 69.4 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.241 | — | — | 8896 | 108.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.159 | — | — | 46322 | 98.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.376 | — | — | 8791 | 105.2 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.103 | — | — | 47546 | 95.0 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.818 | — | — | 8462 | 107.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.033 † | — | — | 30 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.080 † | — | — | 12 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.032 † | — | — | 31 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.217 † | — | — | 5 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.305 | — | — | 9704 | 104.7 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.352 | — | — | 8809 | 148.8 | — | PASS (NO_BASELINE) |
| dump-small | 1.046 | — | — | 95572 | 63.2 | — | PASS (NO_BASELINE) |
| restore-small | 5.965 | — | — | 16766 | 85.2 | — | PASS (NO_BASELINE) |
| dump-big | 2.067 | — | — | 16 | 286.5 | — | PASS (NO_BASELINE) |
| restore-big | 5.584 | — | — | 6 | 418.7 | — | PASS (NO_BASELINE) |
| dump-edge | 5.037 | — | — | 1872 | 303.3 | — | PASS (NO_BASELINE) |
| restore-edge | 4.486 | — | — | 2102 | 589.1 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 3.226 | — | — | 30996 | 74.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.440 | — | — | 4664 | 80.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.831 | — | — | 120282 | 76.4 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.877 | — | — | 12695 | 78.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.496 † | — | — | 201464 | 78.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.423 | — | — | 18441 | 77.6 | — | PASS (NO_BASELINE) |

