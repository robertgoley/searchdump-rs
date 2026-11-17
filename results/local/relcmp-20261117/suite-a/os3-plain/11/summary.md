# searchdump sequential speed suite — os3-plain (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T05:16:35-05:00, finished 2026-11-17T05:41:52-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os3-plain (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.774 | — | — | 56376 | 99.3 | — | PASS (NO_BASELINE) |
| restore-none | 12.069 | — | — | 8286 | 99.3 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.832 | — | — | 54579 | 91.8 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.223 | — | — | 7562 | 95.6 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.446 | — | — | 69168 | 108.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 12.086 | — | — | 8274 | 111.6 | — | PASS (NO_BASELINE) |
| copy | 12.496 | — | — | 8002 | 151.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.546 | — | — | 64691 | 83.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 1.960 | — | — | 51018 | 100.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.138 | — | — | 46766 | 114.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.663 | — | — | 37554 | 120.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.121 | — | — | 89175 | 156.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.494 | — | — | 66951 | 177.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.726 | — | — | 137735 | 228.6 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.922 | — | — | 108446 | 242.6 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.670 | — | — | 59869 | 104.1 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.679 | — | — | 8562 | 101.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.675 | — | — | 59704 | 122.8 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.769 | — | — | 7831 | 101.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.612 | — | — | 62034 | 112.2 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.568 | — | — | 8644 | 100.3 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.028 † | — | — | 36 | 27.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.094 † | — | — | 11 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.028 † | — | — | 36 | 27.0 | — | PASS (NO_BASELINE) |
| settings-restore | 0.226 † | — | — | 4 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.740 | — | — | 9311 | 107.5 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.292 | — | — | 8856 | 159.5 | — | PASS (NO_BASELINE) |
| dump-small | 1.064 | — | — | 93962 | 76.2 | — | PASS (NO_BASELINE) |
| restore-small | 7.250 | — | — | 13794 | 79.7 | — | PASS (NO_BASELINE) |
| dump-big | 1.036 | — | — | 32 | 324.1 | — | PASS (NO_BASELINE) |
| restore-big | 3.175 | — | — | 10 | 449.3 | — | PASS (NO_BASELINE) |
| dump-edge | 2.448 | — | — | 3853 | 366.9 | — | PASS (NO_BASELINE) |
| restore-edge | 2.640 | — | — | 3572 | 589.7 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.382 | — | — | 41973 | 74.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.198 | — | — | 4718 | 75.9 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.614 | — | — | 162754 | 76.3 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.403 | — | — | 13509 | 76.8 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.343 † | — | — | 291586 | 76.3 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.005 | — | — | 19978 | 72.9 | — | PASS (NO_BASELINE) |

