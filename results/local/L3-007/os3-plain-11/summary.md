# searchdump sequential speed suite — os3-plain (3.9.0)

**Results:** 37 PASS, 0 FAIL, 0 skipped/not-run, 37 NO_BASELINE

- git `2c61e55d75630e84c32125d0a7d97b980eb82348`
- started 2026-11-17T16:33:30-05:00, finished 2026-11-17T16:41:09-05:00
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
| dump-none | 0.632 | — | — | 158104 | 62.1 | — | PASS (NO_BASELINE) |
| restore-none | 3.631 | — | — | 27539 | 113.7 | — | PASS (NO_BASELINE) |
| dump-gzip | 0.618 | — | — | 161897 | 98.0 | — | PASS (NO_BASELINE) |
| restore-gzip | 3.662 | — | — | 27304 | 125.4 | — | PASS (NO_BASELINE) |
| dump-zstd | 0.576 | — | — | 173601 | 75.0 | — | PASS (NO_BASELINE) |
| restore-zstd | 3.648 | — | — | 27410 | 113.9 | — | PASS (NO_BASELINE) |
| copy | 3.732 | — | — | 26793 | 149.9 | — | PASS (NO_BASELINE) |
| dump-scroll-s1 | 0.590 | — | — | 169621 | 59.8 | — | PASS (NO_BASELINE) |
| dump-pit-s1 | 0.737 | — | — | 135699 | 66.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s2 | 0.724 | — | — | 138062 | 92.1 | — | PASS (NO_BASELINE) |
| dump-pit-s2 | 0.788 | — | — | 126897 | 104.3 | — | PASS (NO_BASELINE) |
| dump-scroll-s4 | 0.411 † | — | — | 243582 | 134.3 | — | PASS (NO_BASELINE) |
| dump-pit-s4 | 0.435 † | — | — | 229842 | 142.2 | — | PASS (NO_BASELINE) |
| dump-scroll-s8 | 0.288 † | — | — | 347209 | 178.3 | — | PASS (NO_BASELINE) |
| dump-pit-s8 | 0.360 † | — | — | 278138 | 182.2 | — | PASS (NO_BASELINE) |
| dump-split-none | 0.565 | — | — | 176897 | 84.2 | — | PASS (NO_BASELINE) |
| restore-split-none | 4.625 | — | — | 21619 | 103.4 | — | PASS (NO_BASELINE) |
| dump-split-gzip | 0.603 | — | — | 165801 | 82.9 | — | PASS (NO_BASELINE) |
| restore-split-gzip | 4.131 | — | — | 24208 | 103.7 | — | PASS (NO_BASELINE) |
| dump-split-zstd | 0.557 | — | — | 179604 | 107.4 | — | PASS (NO_BASELINE) |
| restore-split-zstd | 4.555 | — | — | 21953 | 78.7 | — | PASS (NO_BASELINE) |
| mapping-dump | 0.011 † | — | — | 92 | 24.0 | — | PASS (NO_BASELINE) |
| mapping-restore | 0.026 † | — | — | 39 | 24.0 | — | PASS (NO_BASELINE) |
| settings-dump | 0.010 † | — | — | 99 | 24.1 | — | PASS (NO_BASELINE) |
| settings-restore | 0.044 † | — | — | 22 | 24.1 | — | PASS (NO_BASELINE) |
| dump-small | 0.318 † | — | — | 314455 | 32.4 | — | PASS (NO_BASELINE) |
| restore-small | 1.854 | — | — | 53944 | 65.4 | — | PASS (NO_BASELINE) |
| dump-big | 0.713 | — | — | 46 | 261.7 | — | PASS (NO_BASELINE) |
| restore-big | 2.159 | — | — | 15 | 436.4 | — | PASS (NO_BASELINE) |
| dump-edge | 1.350 | — | — | 6984 | 384.1 | — | PASS (NO_BASELINE) |
| restore-edge | 1.275 | — | — | 7394 | 583.1 | — | PASS (NO_BASELINE) |
| multi-dump-p1 | 1.133 | — | — | 88228 | 55.5 | — | PASS (NO_BASELINE) |
| multi-restore-p1 | 6.884 | — | — | 14527 | 77.5 | — | PASS (NO_BASELINE) |
| multi-dump-p4 | 0.348 † | — | — | 287177 | 61.5 | — | PASS (NO_BASELINE) |
| multi-restore-p4 | 2.087 | — | — | 47904 | 75.9 | — | PASS (NO_BASELINE) |
| multi-dump-p8 | 0.219 † | — | — | 456750 | 68.0 | — | PASS (NO_BASELINE) |
| multi-restore-p8 | 1.447 | — | — | 69107 | 75.6 | — | PASS (NO_BASELINE) |

