# searchdump sequential speed suite — os13-plain (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T22:28:33-05:00, finished 2026-11-16T22:51:30-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os13-plain (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.809 | — | — | 55290 | 100.6 | — | PASS (NO_BASELINE) |
| restore-none | 10.355 | — | — | 9657 | 106.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.759 | — | — | 56860 | 118.3 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.285 | — | — | 9723 | 99.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.769 | — | — | 56539 | 104.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.663 | — | — | 9378 | 106.8 | — | PASS (NO_BASELINE) |
| copy | 11.793 | — | — | 8480 | 150.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.885 | — | — | 53056 | 86.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.950 | — | — | 51295 | 113.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.102 | — | — | 90755 | 154.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 0.679 | — | — | 147311 | 215.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.868 | — | — | 53527 | 97.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.202 | — | — | 8927 | 102.1 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.819 | — | — | 54980 | 125.1 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.983 | — | — | 9105 | 98.4 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.714 | — | — | 58351 | 123.4 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.007 | — | — | 9085 | 104.5 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.028 † | — | — | 36 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.071 † | — | — | 14 | 26.7 | — | PASS (NO_BASELINE) |
| settings-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.196 † | — | — | 5 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.659 | — | — | 10353 | 100.7 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.454 | — | — | 9565 | 149.9 | — | PASS (NO_BASELINE) |
| dump-small | 0.928 | — | — | 107796 | 77.8 | — | PASS (NO_BASELINE) |
| restore-small | 5.947 | — | — | 16816 | 84.6 | — | PASS (NO_BASELINE) |
| dump-big | 1.049 | — | — | 32 | 322.2 | — | PASS (NO_BASELINE) |
| restore-big | 5.419 | — | — | 6 | 432.1 | — | PASS (NO_BASELINE) |
| dump-edge | 2.518 | — | — | 3746 | 331.7 | — | PASS (NO_BASELINE) |
| restore-edge | 4.423 | — | — | 2132 | 597.0 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.194 | — | — | 45580 | 72.7 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 22.241 | — | — | 4496 | 71.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.564 | — | — | 177204 | 71.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.485 | — | — | 13360 | 68.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.356 † | — | — | 280811 | 73.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.079 | — | — | 19689 | 73.3 | — | PASS (NO_BASELINE) |

