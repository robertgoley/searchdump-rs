# searchdump sequential speed suite — os3-secure (3.9.0)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T07:13:43-05:00, finished 2026-11-17T07:43:47-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os3-secure (3.9.0)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.996 | — | — | 50095 | 72.5 | — | PASS (NO_BASELINE) |
| restore-none | 13.095 | — | — | 7636 | 98.9 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.014 | — | — | 49642 | 82.7 | — | PASS (NO_BASELINE) |
| restore-gzip | 13.201 | — | — | 7575 | 108.0 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.931 | — | — | 51776 | 83.8 | — | PASS (NO_BASELINE) |
| restore-zstd | 13.340 | — | — | 7496 | 101.4 | — | PASS (NO_BASELINE) |
| copy | 14.098 | — | — | 7093 | 129.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.986 | — | — | 50365 | 63.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.441 | — | — | 40972 | 64.5 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.327 | — | — | 42969 | 112.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.845 | — | — | 35144 | 106.4 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.203 | — | — | 83127 | 162.8 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.598 | — | — | 62578 | 150.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.895 | — | — | 111704 | 223.1 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 1.125 | — | — | 88855 | 203.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 2.008 | — | — | 49800 | 80.5 | — | PASS (NO_BASELINE) |
| restore-split-none | 12.933 | — | — | 7732 | 102.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.899 | — | — | 52663 | 96.6 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.821 | — | — | 7800 | 98.9 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.819 | — | — | 54972 | 102.0 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 12.969 | — | — | 7711 | 101.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.026 † | — | — | 39 | 27.1 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.109 † | — | — | 9 | 26.4 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 39 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.274 † | — | — | 4 | 27.3 | — | PASS (NO_BASELINE) |
| restore-norefresh | 14.614 | — | — | 6843 | 92.6 | — | PASS (NO_BASELINE) |
| copy-norefresh | 12.997 | — | — | 7694 | 135.5 | — | PASS (NO_BASELINE) |
| dump-small | 1.027 | — | — | 97328 | 59.1 | — | PASS (NO_BASELINE) |
| restore-small | 8.432 | — | — | 11859 | 82.2 | — | PASS (NO_BASELINE) |
| dump-big | 2.035 | — | — | 16 | 244.0 | — | PASS (NO_BASELINE) |
| restore-big | 4.355 | — | — | 8 | 438.5 | — | PASS (NO_BASELINE) |
| dump-edge | 3.480 | — | — | 2710 | 311.9 | — | PASS (NO_BASELINE) |
| restore-edge | 3.130 | — | — | 3013 | 507.5 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.532 | — | — | 39490 | 74.6 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 23.366 | — | — | 4280 | 80.3 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.630 | — | — | 158627 | 72.0 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.918 | — | — | 12629 | 76.6 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.397 † | — | — | 251773 | 72.4 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 17.074 | — | — | 5857 | 80.5 | — | PASS (NO_BASELINE) |

