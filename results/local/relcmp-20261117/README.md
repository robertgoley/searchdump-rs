# Node / 1.0.0 / 1.1.0 / 1.2.0 comparison (kvmsvr1, 2026-11-16/17)

Host `kvmsvr1` (56 cores, 121 GB RAM). Published GNU release binaries, checksums verified:

- Node elasticdump 6.125.1
- searchdump 1.0.0 (`v1.0.0`, `1d0ac2d`)
- searchdump 1.1.0 (`v1.1.0`, `47a73cc`)
- searchdump 1.2.0 (`v1.2.0`, `5e2ee85`)

`edgen` and `edverify` were built from `5e2ee85` and used for every column. They are not the product under test.

Protocol: `project/plan/PERF-PLAN-1.0-1.2.md`. Ratios are Node wall / tool wall unless noted. Do not mix these numbers with older grokbox or published 32-core tables.

## What is in this directory

- `REPORT.md` — per-engine L0 tables, low-memory notes, and the Suite C table.
- `suite-a/<engine>/<10|11|12>/` — `summary.md` and `combined.json` for that pass. `10` is Node plus 1.0.0 (the suite has no node-only switch). `11` and `12` are `--skip-node`.
- `lowheap/` — `lowheap.json` for Node, 1.0.0, 1.1.0, and 1.2.0.
- `suite-c/` — measured-run TSVs from the successful rerun (2,000,000 lines each).
- `driver.log` — start/finish times and exit codes.

Raw dump files and the Podman store were not committed.

## Headline

Suite A, 16 engines, L0, 3 reps, 48/48 exit 0. Median versus Node on cases with wall at least 0.5 s (400 cases): **1.89× / 2.04× / 2.01×** (1.0 / 1.1 / 1.2). No 1.0 or 1.1 case was slower than Node. One 1.2 case was: OpenSearch 3 plain `multi-restore-p8` (7.8 s Node, 17.0 s 1.2, 0.46×).

Suite B, 7 engines at 256m and 384m: Node 24 dump/load steps passed and 16 failed. 1.0.0 passed 12 and failed 25, several taking the node down. 1.1.0 and 1.2.0 (`--serverLimits=auto`) each passed 14/14 cells.

Suite C, ES 7.17.29, 2 shards, force-merged, `--limit 10000`, same index. Versus unsliced Node (scroll 38.10 s, PIT 37.83 s): scroll `--slices 2` is 3.52× / 3.56× / 3.54×; PIT `--slices 16` is 14.8× / 15.0× / 15.7×. Absolute docs/s are below the published 32-core host because Node here is about 53k docs/s, not 210k. The first Suite C attempt mis-dispatched `searchdump-rs` to the Node runner; the numbers here are the rerun after that fix.
