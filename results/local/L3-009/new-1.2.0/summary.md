# searchdump sequential speed suite — elasticsearch8-plain (8.6.1)

**Results:** 1 PASS, 0 FAIL, 0 skipped/not-run, 1 NO_BASELINE

- git `c205454187e1608c8e23fd9e60bf6d13bfc655ab`
- started 2026-11-17T17:23:17-05:00, finished 2026-11-17T17:25:14-05:00
- tier L0, limit 2000, heap 1024 MB, 20 reps
- searchdump sha256 `e0344bbe1840…`
- searchdump-multi sha256 `88f7b92494ec…`

Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).
NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);
NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.
All three are recorded for comparison only and never fail a searchdump case.
Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.

## searchdump × elasticsearch8-plain (8.6.1)

| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |
|---|---:|---:|---:|---:|---:|---:|---|
| multi-restore-p8 | 1.820 | — | — | 54979 | 51.2 | — | PASS (NO_BASELINE) |

