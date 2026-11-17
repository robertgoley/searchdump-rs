# searchdump sequential speed suite — es710-plain (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T04:35:58-05:00, finished 2026-11-16T05:00:09-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es710-plain (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.468 | — | — | 68134 | 107.7 | — | PASS (NO_BASELINE) |
| restore-none | 8.889 | — | — | 11250 | 103.1 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.230 | — | — | 81329 | 116.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 9.027 | — | — | 11078 | 113.2 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.403 | — | — | 71260 | 108.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 8.587 | — | — | 11646 | 109.3 | — | PASS (NO_BASELINE) |
| copy | 9.438 | — | — | 10596 | 150.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.437 | — | — | 69598 | 93.9 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.344 | — | — | 74388 | 96.2 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.369 | — | — | 73039 | 86.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 1.372 | — | — | 72894 | 87.9 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.256 | — | — | 79606 | 101.7 | — | PASS (NO_BASELINE) |
| restore-split-none | 9.427 | — | — | 10607 | 105.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.233 | — | — | 81103 | 129.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 9.479 | — | — | 10550 | 105.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.338 | — | — | 74735 | 137.0 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 9.392 | — | — | 10648 | 103.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.024 † | — | — | 42 | 27.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.071 † | — | — | 14 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.023 † | — | — | 43 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.184 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 8.355 | — | — | 11969 | 103.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 8.908 | — | — | 11225 | 160.3 | — | PASS (NO_BASELINE) |
| dump-small | 0.847 | — | — | 118078 | 73.8 | — | PASS (NO_BASELINE) |
| restore-small | 5.789 | — | — | 17276 | 82.0 | — | PASS (NO_BASELINE) |
| dump-big | 1.019 | — | — | 32 | 292.2 | — | PASS (NO_BASELINE) |
| restore-big | 5.254 | — | — | 6 | 432.0 | — | PASS (NO_BASELINE) |
| dump-edge | 3.586 | — | — | 2630 | 300.3 | — | PASS (NO_BASELINE) |
| restore-edge | 3.465 | — | — | 2722 | 584.6 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.083 | — | — | 48013 | 76.4 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 20.857 | — | — | 4794 | 78.1 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.565 | — | — | 177110 | 71.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.275 | — | — | 13745 | 75.3 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.318 † | — | — | 314908 | 71.9 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.063 | — | — | 19753 | 71.4 | — | PASS (NO_BASELINE) |

