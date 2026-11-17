# searchdump sequential speed suite — elasticsearch-secure (9.1.2)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T20:50:11-05:00, finished 2026-11-16T21:18:06-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × elasticsearch-secure (9.1.2)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 2.396 | — | — | 41733 | 74.6 | — | PASS (NO_BASELINE) |
| restore-none | 11.162 | — | — | 8959 | 105.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.313 | — | — | 43227 | 88.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.867 | — | — | 9202 | 104.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 2.344 | — | — | 42655 | 90.9 | — | PASS (NO_BASELINE) |
| restore-zstd | 11.130 | — | — | 8985 | 119.7 | — | PASS (NO_BASELINE) |
| copy | 11.326 | — | — | 8829 | 132.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 2.338 | — | — | 42777 | 74.6 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.524 | — | — | 39614 | 78.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.080 | — | — | 48072 | 102.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 1.368 | — | — | 73087 | 109.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.126 | — | — | 88777 | 129.4 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.752 | — | — | 133060 | 171.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.687 | — | — | 145584 | 183.8 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.464 † | — | — | 215428 | 249.9 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.357 | — | — | 42429 | 74.6 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.062 | — | — | 9040 | 112.5 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 2.276 | — | — | 43928 | 96.7 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 11.460 | — | — | 8726 | 107.0 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 2.337 | — | — | 42787 | 96.8 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.000 | — | — | 9091 | 113.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 27.3 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.120 † | — | — | 8 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 38 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.378 † | — | — | 3 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.835 | — | — | 10168 | 106.2 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.420 | — | — | 8756 | 132.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.247 | — | — | 80173 | 64.4 | — | PASS (NO_BASELINE) |
| restore-small | 6.018 | — | — | 16617 | 87.0 | — | PASS (NO_BASELINE) |
| dump-big | 1.140 | — | — | 29 | 291.0 | — | PASS (NO_BASELINE) |
| restore-big | 3.927 | — | — | 8 | 418.0 | — | PASS (NO_BASELINE) |
| dump-edge | 2.482 | — | — | 3800 | 321.0 | — | PASS (NO_BASELINE) |
| restore-edge | 3.549 | — | — | 2657 | 586.2 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.496 | — | — | 40065 | 73.5 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 31.477 | — | — | 3177 | 78.7 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.642 | — | — | 155675 | 79.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 10.790 | — | — | 9268 | 80.1 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.366 † | — | — | 273363 | 76.9 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 8.584 | — | — | 11650 | 79.1 | — | PASS (NO_BASELINE) |

