# searchdump sequential speed suite — es717-secure (7.17.29)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T10:04:03-05:00, finished 2026-11-16T10:31:22-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × es717-secure (7.17.29)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.404 | — | — | 41590 | 73.7 | — | PASS (NO_BASELINE) |
| restore-none | 10.293 | — | — | 9715 | 104.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.214 | — | — | 45167 | 80.9 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.420 | — | — | 9597 | 105.4 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.334 | — | — | 42839 | 86.1 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.515 | — | — | 9510 | 108.8 | — | PASS (NO_BASELINE) |
| copy | 11.145 | — | — | 8973 | 130.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.304 | — | — | 43398 | 73.2 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.565 | — | — | 38980 | 69.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 1.980 | — | — | 50500 | 106.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.404 | — | — | 71220 | 105.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.174 | — | — | 85168 | 137.2 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.759 | — | — | 131690 | 175.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.713 | — | — | 140165 | 221.0 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.458 † | — | — | 218247 | 253.6 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.303 | — | — | 43428 | 80.8 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.474 | — | — | 8716 | 100.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.128 | — | — | 46998 | 104.5 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.149 | — | — | 8231 | 105.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.188 | — | — | 45697 | 99.3 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.029 | — | — | 8313 | 106.8 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 39 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.075 † | — | — | 13 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.025 † | — | — | 40 | 27.3 | — | PASS (NO_BASELINE) |
| settings-restore | 0.214 † | — | — | 5 | 27.3 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.131 | — | — | 9870 | 104.8 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.650 | — | — | 9389 | 133.0 | — | PASS (NO_BASELINE) |
| dump-small | 1.084 | — | — | 92251 | 54.8 | — | PASS (NO_BASELINE) |
| restore-small | 6.175 | — | — | 16196 | 85.1 | — | PASS (NO_BASELINE) |
| dump-big | 1.750 | — | — | 19 | 276.1 | — | PASS (NO_BASELINE) |
| restore-big | 5.542 | — | — | 6 | 438.1 | — | PASS (NO_BASELINE) |
| dump-edge | 3.481 | — | — | 2710 | 335.0 | — | PASS (NO_BASELINE) |
| restore-edge | 4.481 | — | — | 2104 | 528.0 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.733 | — | — | 36583 | 77.3 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 25.863 | — | — | 3866 | 80.6 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.704 | — | — | 142080 | 76.7 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.395 | — | — | 9620 | 78.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.388 † | — | — | 257439 | 72.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 6.760 | — | — | 14793 | 77.8 | — | PASS (NO_BASELINE) |

