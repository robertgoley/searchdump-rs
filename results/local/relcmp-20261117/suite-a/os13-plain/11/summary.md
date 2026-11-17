# searchdump sequential speed suite — os13-plain (1.3.20)

**Results:** 35 PASS, 0 FAIL, 4 skipped/not-run, 35 NO_BASELINE

- git `5e2ee859c05b4b78cb018b59211c35f2b71e3ca1`
- started 2026-11-16T22:06:32-05:00, finished 2026-11-16T22:28:32-05:00
- tier L0, limit 2000, heap 1024 MB, 3 reps
- searchdump sha256 `06233a2a65bc…`
- searchdump-multi sha256 `fe327f433ff3…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.
† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only.

## searchdump × os13-plain (1.3.20)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| dump-none | 1.857 | — | — | 53860 | 87.3 | — | PASS (NO_BASELINE) |
| restore-none | 10.151 | — | — | 9851 | 110.5 | — | PASS (NO_BASELINE) |
| dump-gzip | 1.725 | — | — | 57959 | 118.2 | — | PASS (NO_BASELINE) |
| restore-gzip | 10.249 | — | — | 9757 | 113.1 | — | PASS (NO_BASELINE) |
| dump-zstd | 1.719 | — | — | 58174 | 106.6 | — | PASS (NO_BASELINE) |
| restore-zstd | 10.060 | — | — | 9940 | 111.6 | — | PASS (NO_BASELINE) |
| copy | 10.466 | — | — | 9554 | 143.8 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 1.820 | — | — | 54934 | 103.4 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s2 | 1.932 | — | — | 51750 | 125.8 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s4 | 1.001 | — | — | 99946 | 158.0 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-scroll-s8 | 0.639 | — | — | 156477 | 206.4 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | — | — | — | — | — | — | SKIP: conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3) |
| dump-split-none | 1.794 | — | — | 55756 | 88.9 | — | PASS (NO_BASELINE) |
| restore-split-none | 10.612 | — | — | 9423 | 101.7 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 1.595 | — | — | 62691 | 127.3 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 10.749 | — | — | 9303 | 105.3 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 1.701 | — | — | 58782 | 115.6 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 10.748 | — | — | 9304 | 102.0 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.025 † | — | — | 40 | 27.2 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.067 † | — | — | 15 | 27.2 | — | PASS (NO_BASELINE) |
| settings-dump | 0.026 † | — | — | 39 | 27.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.195 † | — | — | 5 | 27.2 | — | PASS (NO_BASELINE) |
| restore-norefresh | 9.320 | — | — | 10730 | 106.3 | — | PASS (NO_BASELINE) |
| copy-norefresh | 10.129 | — | — | 9873 | 153.7 | — | PASS (NO_BASELINE) |
| dump-small | 0.935 | — | — | 106939 | 76.2 | — | PASS (NO_BASELINE) |
| restore-small | 5.985 | — | — | 16709 | 83.3 | — | PASS (NO_BASELINE) |
| dump-big | 1.073 | — | — | 31 | 296.2 | — | PASS (NO_BASELINE) |
| restore-big | 5.154 | — | — | 6 | 433.5 | — | PASS (NO_BASELINE) |
| dump-edge | 2.786 | — | — | 3385 | 307.6 | — | PASS (NO_BASELINE) |
| restore-edge | 3.772 | — | — | 2500 | 597.7 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 2.108 | — | — | 47434 | 73.8 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 21.986 | — | — | 4548 | 73.8 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.567 | — | — | 176432 | 71.6 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 7.584 | — | — | 13186 | 77.2 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.337 † | — | — | 297048 | 71.6 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 5.075 | — | — | 19704 | 77.6 | — | PASS (NO_BASELINE) |

