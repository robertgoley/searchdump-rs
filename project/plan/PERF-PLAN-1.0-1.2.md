# Performance plan: Node, 1.0, 1.1, 1.2

Status: plan only. Not started. Written 2026-11-16 from `main` at `3fc73e5` after reading the 1.0, 1.1 and 1.2 notes. Do not fill the result tables from older runs: those hosts, timers and missing columns are not this comparison.

## 1. What this measures

Three suites, one host, four tools, in this order on every engine:

1. **Node** elasticdump **6.125.1** (the pinned baseline).
2. **1.0.0** tag `v1.0.0` (`1d0ac2d`). Program source matches `v1.0.0-beta1` (`94e4afb`); only the version string differs.
3. **1.1.0** tag `v1.1.0` (`47a73cc`, `Cargo.toml` version `1.1.0`). Includes 1.1-1 through 1.1-10 and 1.1-4a (slice cap defaults to ES 7.10 only).
4. **1.2 candidate** `origin/release/1.2` at `1f0901e` (not tagged; `Cargo.toml` still says `1.1.0`). This is the tree with `--serverLimits`. Re-pin the SHA if that branch moves before the run.

`edgen` and `edverify` are built once from this checkout and used for every column. They are not the product under test. The four product binaries are never substituted for each other.

### Suite A — regular L0 speed suite

The frozen case list in `scripts/perf/suite.py` / `scripts/perf/README.md`, plus the two 1.1 A/B cases (`PERF_AB_CASES=1`: `restore-norefresh`, `copy-norefresh`). Both tools that can run a case use the **same flags** (`--limit`, `--concurrency=1`, codec, slices). Multi cases pass `--noDefaultDataFlags`, so this suite does **not** measure 1.1-10 or `--serverLimits`. It measures code speed at equal flags.

**1.3 decision 6 (2026-11-17):** for `searchdump` binaries that have `--serverLimits` (1.2 and later, detected from `--help`), the single-index **data** cases (dump/restore/copy, codecs, split, `--slices`, PIT, small/big/edge) also pass `--serverLimits=off`, so the start-up probe (~0.2-0.4 s per run, L3-007) is not timed. 1.0.0/1.1.0 (no such flag) and Node run unchanged; multi cases (already `--noDefaultDataFlags`) and mapping/settings cases are unchanged. The result JSON records `rs_server_limits_off`. Earlier 1.2 Suite A sliced-dump rows include the probe; compare them with that in mind.

L0, place `box`, 3 measured reps after 1 untimed warm-up, one engine at a time. Engine order is the suite order: `es710`, `es717`, `elasticsearch8`, `es819`, `elasticsearch`, `os13`, `os2`, `os3`, each plain then secure (16 profiles). Heap is the suite's: 1024 MB, except OpenSearch 2 and 3 at 4096 MB.

### Suite B — low-memory restrained tests (1.1-10 and 1.2)

`scripts/load/lowheap.py` as extended on `release/1.2` (L-818, L2-006). These runs use **each tool's own defaults**, which is the point of the restraint test. A node that dies is a result, not a skipped cell.

| Tool | How it is invoked |
|---|---|
| Node 6.125.1 | `multielasticdump` defaults (the 1.0-sized pages: about `--limit=5000`, no response cap, large bulks). This is the baseline. |
| 1.0.0 | `searchdump-multi` with the pre-1.1-10 flags (`--limit=5000 --maxResponseMb=0`, load `--limit=5000 --maxBulkBytes=100mb`). |
| 1.1.0 | `searchdump-multi` with no extra sizing flags (1.1-10 defaults: `--limit=1000`, 16 MB bulks, response cap kept). |
| 1.2 | `searchdump-multi` with `--serverLimits=auto` (the 1.2 default). Do not pass `off`; that would just repeat 1.1. |

Cells: engines `es710`, `es861`, `es819`, `es912`, `os13`, `os2`, `os3` at heaps `256m` and `384m`, datasets `pmc` (20,000 docs, ~22 KB) and `fat` (3,000 docs, ~80 KB), operations multi dump and multi load. That is 7 × 2 × 2 × 2 = 56 operations per tool. OpenSearch at 384m stays in the plan; if an image will not boot, record `NO_BOOT` and continue. Seed with small pages (`--limit=200` pmc, `20` fat) outside the measured interval, the same way `lowheap.py` already does.

### Suite C — upstream 2M-doc sliced retrieval

`scripts/benchmark/compare-with-elasticdump.sh`, the maintainer benchmark that produced the original elasticdump-rs sliced table. This is additional to suites A and B. It is not the L0 suite, and L0 sliced rows must not be substituted for it.

Data and engine, unchanged from that script:

- 2,000,000 synthetic log documents, ~256-byte analyzed `message`, plus the script's keyword/date/integer fields.
- One index, 2 primary shards, 0 replicas, default codec, `refresh_interval: -1`, force-merged to 1 segment before any timed run.
- Page size `--limit 10000` for every tool. Scroll keepalive `10m`, PIT keep-alive `10m`.
- Engine: pinned official Elasticsearch 7.17.29 (`es717-plain` image), not the L0 1 GB heap. Use a 4 GB heap and no container CPU cap, so the tool and the slice count are what change. If the first seed hits a circuit breaker, raise the heap to 8 GB and reseed once; record which heap was used.
- Seed once. All four columns read that same index. Do not reseed between tools. The script as written deletes and reseeds on every invocation and always runs Node and one Rust binary together; the driver must gain `BENCH_REUSE_INDEX=1` and `BENCH_TOOLS=node|rs` (or an equivalent wrapper) before this suite is timed. Refusing to delete a named index is already in the script (`BENCH_INDEX_NAME` without `BENCH_ALLOW_DELETE`).

Node has no `--slices`. Its column is the unsliced Node run of the same search type. A sliced release cell is therefore sliced searchdump versus unsliced Node, which is how the original table is built. Label those rows `sliced vs unsliced Node`. Do not describe them as equal-flags.

Required rows, each tool that can run them, 1 untimed warm-up and mean of 2 measured runs (the script default; also keep the two walls so a median can be checked):

| Row | Node | 1.0 / 1.1 / 1.2 |
|---|---|---|
| scroll, unsliced, no pin | yes | yes |
| PIT, unsliced, no pin | yes | yes |
| scroll, unsliced, pinned to 1 core | yes | yes |
| PIT, unsliced, pinned to 1 core | yes | yes |
| scroll, unsliced, pinned to 4 cores | yes | yes |
| PIT, unsliced, pinned to 4 cores | yes | yes |
| scroll, `--slices 2` (workers matched), no pin | unsliced Node scroll | yes |
| PIT, `--slices 16` (workers matched), no pin | unsliced Node PIT | yes |

ES 7.17 is not ES 7.10, so 1.1 `--sliceCap=auto` does not lower these slice counts. Scroll `--slices 2` matches the 2 shards. PIT `--slices 16` is never capped. Record docs/s, MiB/s or GiB/s, wall seconds, and CPU-seconds. Ratio is Node wall / tool wall, and a second ratio Node CPU-s / tool CPU-s (higher means less CPU than Node). Node's own column is `1.00×`.

Order on that one index: Node unsliced rows first, then 1.0, then 1.1, then 1.2. Sliced rows for a release run immediately after that release's unsliced rows, before the next binary.

## 2. Result table (the required end shape)

One row per engine and case. Four columns. Every release number is **Node wall / tool wall** (higher means faster than Node), with the median wall in seconds beside it. Node's own column is `1.00×` when it produced a time.

```
| Engine | Case | Node s | 1.0.0 | 1.1.0 | 1.2 |
|---|---|---:|---:|---:|---:|
| es710-plain | dump-none | 3.547 | 1.473s 2.41× | … | … |
```

Rules:

- Ratio uses the median of the 3 reps. Warm-up is not in the median.
- A release cell is filled only when that run passed the suite's correctness gate (exit 0, digests equal, no failed writes). Otherwise the cell is `FAIL` and is not given a ratio.
- If Node did not produce a time, the ratio cells are `—` and the release walls are still printed. Do not invent a Node baseline. Suite A Rust-only cases (zstd, `--slices`, PIT) stay in a second table whose baseline is **1.0.0 wall**, labeled `× vs 1.0`. Suite C is the exception: sliced rows use unsliced Node of the same search type as the baseline, and are labeled `sliced vs unsliced Node`.
- PIT rows on ES 7.10 and OpenSearch 1.3 are `SKIP` (the suite's cross-distribution exclusion), not failures.
- Walls under 0.5 s are marked `†` and stay out of the headline median.
- Low-memory table uses the same four columns. A dead node is `OOM` or `FAIL`, not a time. A release ratio is filled only when **both** Node and that release passed. When Node dies and the release passes, the cell is `PASS` plus wall, and the ratio stays `—`.

Headline summary, filled only from the Node-compared rows that all four tools passed:

| Group | Node | 1.0.0 | 1.1.0 | 1.2 |
|---|---:|---:|---:|---:|
| all compared cases (median ×) | 1.00× | | | |
| dump none/gzip | 1.00× | | | |
| restore none/gzip | 1.00× | | | |
| copy | 1.00× | | | |
| split dump/restore | 1.00× | | | |
| multi p1 / p4 / p8 | 1.00× | | | |
| low-mem dump+load, cells both passed | 1.00× | | | |
| 2M scroll unsliced | 1.00× | | | |
| 2M PIT unsliced | 1.00× | | | |
| 2M scroll `--slices 2` vs unsliced Node | 1.00× | | | |
| 2M PIT `--slices 16` vs unsliced Node | 1.00× | | | |

Plus a pass-count line for low-mem: `Node n/56, 1.0 n/56, 1.1 n/56, 1.2 n/56`.

Suite C detail table, one row per mode. Cells are `docs/s · MiB/s · wall × · CPU ×`. The original published point (PIT 16 at 2,030,457 docs/s, 9.66×, 2.7× less CPU; scroll 2 at 788,955 docs/s, 3.77×, 5.4× less CPU) is a different host and is not filled in here.

```
| Mode | Node | 1.0.0 | 1.1.0 | 1.2 |
|---|---|---|---|---|
| scroll, unsliced | docs/s · MiB/s | × wall · × CPU | | |
| PIT, unsliced | docs/s · MiB/s | | | |
| scroll, 1 core | | | | |
| PIT, 1 core | | | | |
| scroll, 4 cores | | | | |
| PIT, 4 cores | | | | |
| scroll, --slices 2 | unsliced Node | sliced vs that Node | | |
| PIT, --slices 16 | unsliced Node | sliced vs that Node | | |
```

## 3. Case list (suite A)

Node-compared (ratio vs Node): `dump-none`, `restore-none`, `dump-gzip`, `restore-gzip`, `copy`, `dump-split-none`, `restore-split-none`, `dump-split-gzip`, `restore-split-gzip`, `dump-small`, `restore-small`, `dump-big`, `restore-big`, `dump-edge`, `restore-edge`, `mapping-dump`, `mapping-restore`, `settings-dump`, `settings-restore`, `multi-dump-p1`, `multi-restore-p1`, `multi-dump-p4`, `multi-restore-p4`, `multi-dump-p8`, `multi-restore-p8`, `restore-norefresh`, `copy-norefresh`.

Rust-only (ratio vs 1.0.0, separate table): `dump-zstd`, `restore-zstd`, `dump-split-zstd`, `restore-split-zstd`, `dump-scroll-s1`, `dump-scroll-s2`, `dump-scroll-s4`, `dump-scroll-s8`, `dump-pit-s1`, `dump-pit-s2`, `dump-pit-s4`, `dump-pit-s8`.

Datasets and limits are the suite's L0 values: logs/small/many 100,000 docs at `--limit=2000`, big 33 docs at `--limit=10`, edge 10,000 generated. Do not change them.

## 4. How to run

Host: this machine (`kvmsvr1`) unless Robert names another. Isolated Podman store (do not prune the host store). One engine at a time. Port `19232` for suite A, `19413` for suite B. `STOP` file between engines aborts cleanly. No other engine on those ports. Quiet host; record `PERF_HOST_ID=kvmsvr1-relcmp`.

Build, once, before any timed run:

```bash
# Node 6.125.1, ignore-scripts, from scripts/perf/node-package-lock.json
# 1.0.0, 1.1.0, 1.2 candidate: cargo +1.91.1 build --locked --release --bins
#   of searchdump, searchdump-multi; copy edgen and edverify from this checkout
```

Record SHA-256 of the four product binaries and `elasticdump --version` (must be `6.125.1`) in the report header.

Suite A, per engine, back to back, fresh engine each tool so a dead node cannot leak into the next column:

```bash
# 1. Node. The suite has no node-only switch. Run it with PERF_BIN_DIR pointing at
#    the 1.0 binaries and keep only the Node rows from that pass. The 1.0 rows from
#    this same pass are column 2 (same engine generation, same seed).
PERF_BIN_DIR=$BIN10 PERF_AB_CASES=1 PERF_HOST_ID=kvmsvr1-relcmp-node \
  bash scripts/perf/run-suite.sh --host-tools --tier L0 --place box --reps 3 \
  --engines "$ENGINE"

# 2. 1.1 and 1.2 are --skip-node, same engine id, immediately after.
PERF_BIN_DIR=$BIN11 PERF_AB_CASES=1 PERF_HOST_ID=kvmsvr1-relcmp-11 \
  bash scripts/perf/run-suite.sh --host-tools --tier L0 --place box --reps 3 \
  --skip-node --engines "$ENGINE"
PERF_BIN_DIR=$BIN12 PERF_AB_CASES=1 PERF_HOST_ID=kvmsvr1-relcmp-12 \
  bash scripts/perf/run-suite.sh --host-tools --tier L0 --place box --reps 3 \
  --skip-node --engines "$ENGINE"
```

Column 1.0 comes from the Rust rows of the first pass, not from a fourth engine start. That keeps Node and 1.0 on one seed. 1.1 and 1.2 each get a fresh engine, which is the suite's normal isolation. Say so in the report. Do not compare a 1.1 row to a Node row from a different engine profile.

Suite B, one engine and heap at a time, tool order Node, 1.0, 1.1, 1.2. Restart the node after an OOM before the next tool. Use the 1.2 `lowheap.py` (it has `--mode` and the OpenSearch cells). `--mode none` is the 1.1 binary; `--regression` is the 1.0 flags; `--mode auto` is 1.2. Node is a separate driver that calls `multielasticdump` on the same seeded index and scores the same digest. A Node OOM is recorded and the engine is restarted; it does not fail the release columns.

Suite C, after the es717 image is up on its own port (`19242`) and the 2M index is seeded and force-merged:

```bash
# Seed only. Named index, do not time, do not delete afterwards.
BENCH_INDEX_NAME=searchdump-relcmp-2m BENCH_DOCS=2000000 \
BENCH_WARMUP_RUNS=0 BENCH_MEASURED_RUNS=0 BENCH_KEEP_ARTIFACTS=1 \
  bash scripts/benchmark/compare-with-elasticdump.sh \
  --es-url "$ES717_URL" --rs-bin "$BIN10/searchdump" \
  --elasticdump-cmd "$NODE_ED"

# Node first, unsliced scroll and PIT, no pin, then the 1-core and 4-core pins.
# Then 1.0, 1.1, 1.2 on that same index: unsliced, then --rs-slices 2 --search-types scroll,
# then --rs-slices 16 --search-types pit. Workers match slices. No pin on the sliced rows.
```

Smoke gate before the 16-engine run: `es710-plain` only, suites A and B, all four columns. If that gate is not a clean Node time plus three release passes, stop and fix the harness. Do not start the other 15 engines. Suite C is not part of that gate; it runs once, on the 2M ES 7.17 index, after the gate passes.

## 5. What earlier notes already say (do not paste those numbers into this table)

- **1.0 / beta1** (`scripts/perf/results/v1.0.0-beta1-L0.md`, grokbox): 576 PASS, 0 FAIL, median about 2.01× vs Node over 367 cases. Different host.
- **1.1** (`pit-1.1.md` grokbox, `v1.1-pit-kvmsvr1.md` this host): refresh every 5 s is the restore win (about 1.3–1.5× vs per-batch refresh, not 2–4×). Slice cap helps ES 7.10 over-sliced scroll and slows ES 8.19 / OpenSearch 3, which is why 1.1-4a limits the default cap to ES 7.10. kvmsvr1 Node medians on beta1-equivalent bits were 2.41× / 1.97× / 1.99× on three plain engines only.
- **1.1-10 / low-mem:** 1.0 flags fail most 256m/384m cells (L-818: 23 of 32; L2-006: 20 of 56, 15 taking the node down). 1.1 defaults passed 16/16 ES cells (L-820). 1.2 `auto`/`off`/`min` passed 14/14 engine×heap cells on ES 7.10–9.1 and OpenSearch 1.3/2/3 (L2-006).
- **1.2 speed** (L2-008, other VM, no Node column): median 0.78× the beta1 wall over 573 cases, so faster than 1.0 on that host. Not a Node ratio. Not this host.
- **Upstream sliced table** (tenyears-tech README, 32-core host, this script): PIT `--slices 16` 2,030,457 docs/s, 9.66× Node, 2.7× less CPU; scroll `--slices 2` 788,955 docs/s, 3.77× Node, 5.4× less CPU. Our 1M-doc runs (L-808, L-807) are not that dataset and must not be entered in the Suite C table.

## 6. Time and stop rule

On this host the last 3-engine 1.1 batch was about 30 minutes per engine without Node and about 55 minutes with Node. Sixteen engines × (Node+1.0 together, then 1.1, then 1.2) is on the order of a day and a half if nothing stalls. Suite B is a few hours more. Suite C seeds 2M documents once (bulk 5000, then a force-merge) and then runs 8 modes × 4 tools × 3 passes (1 warm-up + 2 measured). Budget a few hours for that, separate from the L0 day. A `STOP` file halts between engines, and between Suite C modes. Write the partial table from completed engines rather than waiting for all 16.

## 7. Report file

Write `scripts/perf/results/v1.0-1.1-1.2-vs-node.md` only after the run. Header: host, date, four SHAs, binary hashes, Node version, reps, place, heap. Then the headline median table, the low-mem pass counts, the full Node-ratio table, the Rust-only vs-1.0 table, the low-mem table, and the Suite C 2M sliced table (docs/s, MiB/s, wall ×, CPU ×). No number in that file comes from section 5.
