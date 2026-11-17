# searchdump sequential speed suite — elasticsearch-secure (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T20:21:51-05:00, finished 2026-11-16T20:50:10-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch-secure (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.313 | — | — | 43231 | 72.6 | — | PASS (NO_BASELINE) |
| restore-none | 10.750 | — | — | 9302 | 108.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.190 | — | — | 45660 | 84.7 | — | PASS (NO_BASELINE) |
| restore-gzip | 11.230 | — | — | 8905 | 104.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.265 | — | — | 44144 | 97.3 | — | PASS (NO_BASELINE) |
| restore-zstd | 11.121 | — | — | 8992 | 107.4 | — | PASS (NO_BASELINE) |
| copy | 11.607 | — | — | 8616 | 129.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.263 | — | — | 44192 | 82.0 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.577 | — | — | 38800 | 73.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.171 | — | — | 46053 | 102.7 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.289 | — | — | 77558 | 100.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.104 | — | — | 90612 | 130.6 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.804 | — | — | 124370 | 157.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.690 | — | — | 144920 | 208.4 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.462 † | — | — | 216407 | 237.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.298 | — | — | 43509 | 82.9 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.549 | — | — | 8659 | 104.5 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.310 | — | — | 43282 | 104.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.143 | — | — | 8974 | 104.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.319 | — | — | 43130 | 94.3 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.333 | — | — | 8824 | 109.4 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.030 † | — | — | 34 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.121 † | — | — | 8 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.028 † | — | — | 36 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.389 † | — | — | 3 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.544 | — | — | 9484 | 106.9 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.223 | — | — | 8910 | 129.1 | — | PASS (NO_BASELINE) |
| dump-small | 1.225 | — | — | 81600 | 66.6 | — | PASS (NO_BASELINE) |
| restore-small | 5.887 | — | — | 16986 | 92.7 | — | PASS (NO_BASELINE) |
| dump-big | 1.152 | — | — | 29 | 282.7 | — | PASS (NO_BASELINE) |
| restore-big | 3.823 | — | — | 9 | 436.3 | — | PASS (NO_BASELINE) |
| dump-edge | 2.597 | — | — | 3632 | 325.6 | — | PASS (NO_BASELINE) |
| restore-edge | 3.263 | — | — | 2890 | 513.9 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.503 | — | — | 39952 | 75.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 32.322 | — | — | 3094 | 78.7 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.642 | — | — | 155794 | 76.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 11.415 | — | — | 8761 | 77.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.355 † | — | — | 281310 | 78.3 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 7.770 | — | — | 12870 | 77.9 | — | PASS (NO_BASELINE) |

