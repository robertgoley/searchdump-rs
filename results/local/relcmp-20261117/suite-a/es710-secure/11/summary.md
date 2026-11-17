# searchdump sequential speed suite — es710-secure (7.10.2)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T06:15:42-05:00, finished 2026-11-16T06:48:13-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es710-secure (7.10.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.849 | — | — | 54070 | 83.8 | — | PASS (NO_BASELINE) |
| restore-none | 9.784 | — | — | 10221 | 105.4 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.613 | — | — | 62014 | 87.1 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.069 | — | — | 9931 | 107.5 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.873 | — | — | 53404 | 85.7 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.029 | — | — | 9971 | 104.8 | — | PASS (NO_BASELINE) |
| copy | 10.456 | — | — | 9564 | 147.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.198 | — | — | 45487 | 72.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.951 | — | — | 51252 | 68.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 2.135 | — | — | 46842 | 73.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 2.196 | — | — | 45539 | 70.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 2.200 | — | — | 45447 | 74.1 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.455 | — | — | 8730 | 100.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.164 | — | — | 46206 | 102.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.644 | — | — | 8588 | 103.0 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.188 | — | — | 45709 | 91.7 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.159 | — | — | 8224 | 104.6 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.033 † | — | — | 31 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.082 † | — | — | 12 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.033 † | — | — | 30 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.206 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.422 | — | — | 9595 | 100.8 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.570 | — | — | 8643 | 139.5 | — | PASS (NO_BASELINE) |
| dump-small | 1.005 | — | — | 99532 | 61.1 | — | PASS (NO_BASELINE) |
| restore-small | 6.108 | — | — | 16371 | 80.8 | — | PASS (NO_BASELINE) |
| dump-big | 2.038 | — | — | 16 | 267.6 | — | PASS (NO_BASELINE) |
| restore-big | 5.508 | — | — | 6 | 441.8 | — | PASS (NO_BASELINE) |
| dump-edge | 4.625 | — | — | 2039 | 300.7 | — | PASS (NO_BASELINE) |
| restore-edge | 4.106 | — | — | 2297 | 526.0 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.981 | — | — | 33545 | 72.3 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.158 | — | — | 4726 | 82.2 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.828 | — | — | 120705 | 71.0 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.943 | — | — | 12590 | 78.0 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.466 † | — | — | 214500 | 72.2 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.474 | — | — | 18268 | 76.1 | — | PASS (NO_BASELINE) |

