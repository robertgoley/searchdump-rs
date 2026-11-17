# searchdump sequential speed suite — os2-plain (2.19.7)

**Results:** 39 PASS, 0 FAIL, 0 skipped/not-run, 39 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-17T02:01:47-05:00, finished 2026-11-17T02:26:34-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os2-plain (2.19.7)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.915 | — | — | 52231 | 90.1 | — | PASS (NO_BASELINE) |
| restore-none | 11.233 | — | — | 8902 | 107.1 | — | PASS (NO_BASELINE) |
| dump-gzip | 2.041 | — | — | 48990 | 92.1 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.577 | — | — | 9455 | 102.7 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.831 | — | — | 54608 | 108.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.605 | — | — | 9430 | 113.8 | — | PASS (NO_BASELINE) |
| copy | 11.943 | — | — | 8373 | 153.7 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.864 | — | — | 53653 | 87.7 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 2.417 | — | — | 41381 | 95.1 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 2.060 | — | — | 48540 | 118.5 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 2.652 | — | — | 37710 | 123.6 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 1.163 | — | — | 86018 | 144.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 1.562 | — | — | 64013 | 181.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.791 | — | — | 126363 | 176.4 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 1.017 | — | — | 98356 | 234.7 | — | PASS (NO_BASELINE) |
| dump-split-none | 1.803 | — | — | 55449 | 100.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 11.438 | — | — | 8742 | 102.0 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.856 | — | — | 53883 | 116.2 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 12.125 | — | — | 8247 | 102.5 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.807 | — | — | 55352 | 117.7 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 11.268 | — | — | 8875 | 104.8 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.027 † | — | — | 37 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.096 † | — | — | 10 | 27.1 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 39 | 27.2 | — | PASS (NO_BASELINE) |
| settings-restore | 0.235 † | — | — | 4 | 27.1 | — | PASS (NO_BASELINE) |
| restore-norefresh | 10.396 | — | — | 9619 | 107.4 | — | PASS (NO_BASELINE) |
| copy-norefresh | 11.053 | — | — | 9047 | 154.2 | — | PASS (NO_BASELINE) |
| dump-small | 1.094 | — | — | 91447 | 70.0 | — | PASS (NO_BASELINE) |
| restore-small | 6.153 | — | — | 16252 | 83.2 | — | PASS (NO_BASELINE) |
| dump-big | 1.144 | — | — | 29 | 280.4 | — | PASS (NO_BASELINE) |
| restore-big | 3.490 | — | — | 10 | 445.6 | — | PASS (NO_BASELINE) |
| dump-edge | 7.445 | — | — | 1267 | 314.1 | — | PASS (NO_BASELINE) |
| restore-edge | 2.849 | — | — | 3310 | 511.9 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.729 | — | — | 36645 | 74.1 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.141 | — | — | 4730 | 75.3 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.683 | — | — | 146412 | 72.1 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.199 | — | — | 13890 | 76.0 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.388 † | — | — | 257478 | 73.7 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 4.856 | — | — | 20592 | 74.1 | — | PASS (NO_BASELINE) |

