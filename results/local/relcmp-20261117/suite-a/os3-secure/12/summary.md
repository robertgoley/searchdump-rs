# searchdump sequential speed suite — os3-secure (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T07:43:48-05:00, finished 2026-11-17T08:15:30-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os3-secure (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.413 | — | — | 41447 | 74.3 | — | PASS (NO_BASELINE) |
| restore-none | 14.806 | — | — | 6754 | 94.8 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.026 | — | — | 49370 | 82.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.666 | — | — | 7317 | 99.4 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.940 | — | — | 51538 | 86.2 | — | PASS (NO_BASELINE) |
| restore-zstd | 13.729 | — | — | 7284 | 112.1 | — | PASS (NO_BASELINE) |
| copy | 14.295 | — | — | 6995 | 126.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.140 | — | — | 46726 | 71.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.570 | — | — | 38904 | 71.0 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.305 | — | — | 43382 | 95.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.764 | — | — | 36178 | 112.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.287 | — | — | 77710 | 145.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.536 | — | — | 65104 | 149.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.836 | — | — | 119639 | 183.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 1.248 | — | — | 80096 | 223.8 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.195 | — | — | 45552 | 75.0 | — | PASS (NO_BASELINE) |
| restore-split-none | 13.603 | — | — | 7351 | 94.8 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.130 | — | — | 46957 | 94.5 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 13.438 | — | — | 7442 | 97.6 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.035 | — | — | 49146 | 103.5 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 13.517 | — | — | 7398 | 105.6 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 26.8 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.120 † | — | — | 8 | 26.4 | — | PASS (NO_BASELINE) |
| settings-dump | 0.030 † | — | — | 34 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.284 † | — | — | 4 | 26.8 | — | PASS (NO_BASELINE) |
| restore-norefresh | 14.166 | — | — | 7059 | 94.3 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.889 | — | — | 7759 | 135.3 | — | PASS (NO_BASELINE) |
| dump-small | 1.062 | — | — | 94168 | 49.2 | — | PASS (NO_BASELINE) |
| restore-small | 7.390 | — | — | 13531 | 89.9 | — | PASS (NO_BASELINE) |
| dump-big | 1.748 | — | — | 19 | 295.4 | — | PASS (NO_BASELINE) |
| restore-big | 3.831 | — | — | 9 | 472.5 | — | PASS (NO_BASELINE) |
| dump-edge | 4.263 | — | — | 2212 | 300.3 | — | PASS (NO_BASELINE) |
| restore-edge | 4.074 | — | — | 2315 | 512.4 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 3.079 | — | — | 32473 | 73.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 30.013 | — | — | 3332 | 80.1 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.713 | — | — | 140216 | 72.8 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.145 | — | — | 9857 | 77.0 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.438 † | — | — | 228488 | 72.8 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.183 | — | — | 19295 | 78.4 | — | PASS (NO_BASELINE) |

