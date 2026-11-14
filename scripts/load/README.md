# Load harness (TST-INF-006, TST-LOAD-001…003, LOAD-012)

For the one-command, all-engine sequential speed/regression and Node comparison suite,
see [../perf/README.md](../perf/README.md). It reuses this harness's measurement, seeding,
verification, resource-tier and comparator helpers; specialized investigations below remain available.

## measure.sh

`measure.sh` runs one command and records its metrics.

```
scripts/load/measure.sh [--run-id ID] [--out FILE] [--cluster URL] [--docs N] [--input P]... [--output P]...
                        [--uncompressed-bytes N] [--label k=v]... -- CMD...
scripts/load/measure.py --validate FILE...      # check a file against metrics.schema.json
scripts/load/compare.py BASE.json NEW.json [--threshold 10]   # exit 1 on a regression
```

It records:

- wall time, user and system CPU;
- max RSS from rusage, and peak RSS, FDs and threads sampled from `/proc` every 100 ms;
- voluntary and involuntary context switches;
- bytes read and written;
- docs/s, and MB/s in and out;
- compression ratio;
- `Total Writes` / `Failed Writes`, and retries by kind, parsed from the tool's stderr.

With `--cluster`, it also samples `_nodes/stats` before and after the run. From that it reports:

- indexing and search rates;
- rejected writes and searches;
- open scroll and PIT contexts afterwards.

The output goes to `results/<run-id>.json`, which is git-ignored; schema `edrs-metrics/v1`. A copy of stderr is kept next to it.

**Deviation from the spec:** the spec asks for `/usr/bin/time -v` and a cgroup per command. The box has no `/usr/bin/time`, and per-command cgroups need root, or systemd user scopes that the box does not have. RSS therefore comes from `wait4` rusage, which includes waited-for children, plus `/proc` sampling.

## run.sh

`run.sh` is the load runner. See the header of `scripts/load/run.sh` and `scenarios/*.toml`.

### Running a scenario

```
scripts/load/run.sh L0 dump-restore-logs                  # box, ES 7.17.29 (scenario default)
scripts/load/run.sh L0 copy-logs --engine os13-plain
scripts/load/run.sh L1 dump-restore-logs --dry-run        # print the fit decision only
LOAD_PLACE=runner scripts/load/run.sh L1R dump-restore-logs
```

**Tiers and places.** These are defined in `scenarios/tiers.toml`. The runner refuses, with exit code 2, a tier that is not allowed at the place, or one whose heap or estimated disk exceeds the place budget. On the box, L2 is never allowed.

**Resource guards.** Before starting, the runner takes the shared lock `/tmp/searchdump-it.lock`. It then checks memory (MemAvailable ≥ 2×heap + 512 MB + tool budget) and disk.

**Seeding.** The runner seeds each index from `edgen` through its own `_bulk` client. Edge-case documents that the engine rejects are recorded per reason in `seeds[].reject_reasons`, together with `seeds[].overwritten`; they are not counted as failures. The reference for verification is always the digest of what the engine stored.

**Correctness (LOAD-002).** After every step, `edverify diff --index-as x` compares the source index with the step's output (a dump file or a target index). Any mismatch fails the run, and the failure names the first 10 differing ids. L0 verification uses one scroll slice with 100 docs per page to fit the unchanged 1 GB heap; other tiers keep 4 slices / 1000 docs. Override with `--verify-size N --verify-slices N` (positive values); the selected limits are recorded in result JSON under `verification`. edverify retries explicit HTTP 429s with bounded backoff and clears its own scroll IDs on success and error; a failed cleanup fails the verification. All documents, digest and leak assertions remain unchanged.

**Metrics (LOAD-003).** Each tool step runs under `measure.sh` and is schema-validated. The tool's max RSS must stay under the place's `tool_mem_mb`. `Total Writes` must equal the source count, and `Failed Writes` must be 0.

**Leak check (LOAD-017).** Every load script (`run.py`, `split.py`, `resume.py`, `faults.py`, `curves.py`, `compress.py`, `allocbench.py`, `nodecmp.py`, `manyidx.py`) calls `run.leak_check` before its teardown and fails the run on a leak (L-811). At the end of a run there must be:

- no open scroll or PIT contexts: `scroll_current` and, on OpenSearch, `point_in_time_current` reach 0 within 15 s. Elasticsearch has no PIT stat, so there `open_contexts` minus the scrolls (`other_open_contexts`, which counts PITs) must reach 0. `nodecmp.py` only records the contexts, because Node elasticdump may leave scrolls open; its Rust runs must leave none;
- no `.part` files in the work directory, hidden staging files (`.name.part-*`) included. Cluster data dirs (`it`, `it-*`) are skipped; `split.py` allows the parts its kill cases leave on purpose;
- no unexpected non-system indices and no new hidden index outside the engines' own prefixes (`run.SYSTEM_INDEX_PREFIXES`), when the script records the indices at the start (`run.py`).

`leakcheck_selftest.py` checks the helper itself: offline stubs, plus one small container (`--engine es717-plain` or `opensearch-plain`) with an open scroll, an open PIT, a stray index and a hidden index; about 15 s per engine.

**Results.** Output is written to `results/load/<run-id>/<scenario>.json` (schema `edrs-load/v1`), alongside the metrics and verify reports. Credentials reach the tools through environment variables only, never through argv.

## matrix.sh: the LOAD-004 scenario matrix

`matrix.sh` runs every scenario listed in `scenarios/matrix.toml` at one tier and place. It gates each cell, which is one step of one scenario.

```
LOAD_WORK=/workspace/logs/l801/work scripts/load/matrix.sh L0 --place box
scripts/load/matrix.sh L1L --place box                    # full L1-lite (1M docs, 2 GB heap)
scripts/load/matrix.sh L0 --place shared                  # scaled down while other agents use the box
scripts/load/matrix.sh L0 --place box --write-baseline    # record the perf baseline
scripts/load/matrix.sh L0 --only matrix-split --docs 5000 # smoke (never compared or recorded)
```

| Scenario | Cells |
|---|---|
| `matrix-dump` | `scroll`, `pit`, `pit-s2`, `pit-s4`, `pit-s8`, `pit-s16` |
| `matrix-restore` | `c{1,4,8}-l{1000,5000}` (`--concurrency` x `--limit`), from one verified dump (`src`) |
| `matrix-copy` | `copy` (scroll), `copy-pit-s4` (cluster -> cluster) |
| `matrix-compress` | `fscompress` (gzip), `zstd-1`, `zstd-3`, `zstd-9` (`--compressionLevel`) |
| `matrix-split` | `filesize` (`--fileSize=20mb`), `maxrows` (`--maxRows=25000`), `filesize-zstd` |
| `matrix-filter` | `full`, `sourceonly` (`--sourceOnly`), `range` (`--searchBody` range on `http.response.status_code`) |

**Correctness gate.** Every cell is checked with `edverify diff`, and the tool must exit 0. Every cell must also pass the scenario's write-count, RSS-budget and leak checks. What a cell is compared against depends on its type:

| Cell type | Compared against |
|---|---|
| Dump | The cluster digest. |
| Split dump | The cluster digest, over all parts together. The cell also needs at least `min_parts` parts. |
| `verify = "source"` | The raw `_source` of every line of the scenario's last full dump. |
| `verify = "query"` | An index built by the engine's own `_reindex` with the same query. `Total Writes` must equal that index's count, and the index is deleted afterwards. |
| Restore and copy | The source index. The target index is deleted after its check. |

**Perf baseline.** Each run's docs/s and tool max RSS are compared with `baselines/<place>-<tier>-<engine>.json`. A cell fails if it is more than `threshold_pct` (30) worse; the threshold is wide because the shared box is noisy. Baselines are only comparable within the same place, tier and engine. A cell that has no baseline entry is reported as `no-baseline`.

The run writes `results/load/<run-id>/matrix.json` (git-ignored).

### Sizes and where they run

| Tier | Docs | Heap | Where it runs |
|---|---|---|---|
| L0 | 100k | 1 GB (768 MB on `shared`) | Everywhere. Full size on this box: place `box`. |
| L1L | 1M | 2 GB | Full L1-lite. On this box with place `box`; needs about 6 GB of free RAM. Also runs on `big`. |
| L1S | 250k | 768 MB | L1-lite scaled down, for place `shared` only (when other agents need the box). |
| L1 | 10M | 4 GB | Phase B, on a dedicated load host (`big`). The memory guard needs about 10 GB free, which this box does not have. |

Committed baselines:

- `baselines/box-L0-es717-plain.json` and `baselines/box-L1L-es717-plain.json` are full size.
- `baselines/shared-L0-es717-plain.json` and `baselines/shared-L1S-es717-plain.json` are scaled down.
- `baselines/shared-L1L-es717-plain.json` has the full 1M docs, with the heap capped at 768 MB.

All 25 cells were correct in every committed baseline run. One outlier showed up: a single shared L1L run took 157 s for `zstd-9`, where the box run and the shared rerun took 10-11 s (an offline file-to-file run takes 10.5 s). It did not reproduce, so the committed value is from the rerun. If a regression shows up on one cell, rerun that scenario with `--only` before treating it as real.

A tier that a place cannot hold is refused with exit 2, and the message says what the tier needs.

### Place `shared`: the scaled-down fallback while other agents use the box

The `shared` place keeps the run to about half the box:

- **CPU.** `cpus = "0-3"` pins `run.py` to 4 of the 8 CPUs, and everything it starts inherits that affinity: the tools, `edverify`, and the engine container (through podman). `IT_CONTAINER_CPUS=2` is passed as well.
- **Memory.** The heap is capped at 768 MB and the JVM runs lean.
- **Disk.** `data_on_work = true` mounts the engine's data dir under `LOAD_WORK` (`IT_ES_DATA_MOUNT=1` in `up.sh`), so all load data lives in the run's work dir and is deleted with it; place `box` does the same. This applies to the official Elasticsearch images, which is the matrix default (`es717-plain`); other engines keep their data in podman's container storage.

Rootless podman with the cgroupfs manager accepts `--memory` and `--cpus` on this box but does not enforce them; inside the container, `memory.max` reads `max` and `nproc` reads 8. Two mechanisms therefore enforce the limits instead:

- **CPU:** the affinity above.
- **Memory:** a watchdog in `run.py` samples the engine's process-tree RSS from the host's `/proc` every 2 s. If it exceeds `node_rss_mb` (1800), the watchdog kills the container and fails the run.

The peak is recorded as `node.peak_rss_mb`.

Split file names follow Node elasticdump: `<base>.split-<n><ext>[.gz|.zst]`. An output that already ends in `.zst` therefore gets the codec suffix twice, so the matrix names split outputs `*.ndjson`.

## Memory checks (LOAD-005, `memcheck.py`)

`memcheck.py` checks that peak RSS is bounded and flat and that the README memory formula holds. It has three modes:

- `memcheck.py matrix SMALL LARGE`: compares two `matrix.sh` runs with the same flags at two sizes, for example `results/load/<L0 run>` and `results/load/<L1L run>`. Each cell passes when its peak RSS at LARGE is at most `--flat-pct` (10) above SMALL and every peak is at most `--formula-pct` (25) above the README estimate. `--formula old` checks the pre-L-802 formula (x 1.5 + 10 MB).
- `memcheck.py files`: runs the file-to-file cases in `scenarios/memcheck.toml` (doc sizes about 300 B, 1 KB, 40 KB and 1 MB) at a small and a large document count. It needs no cluster; data goes under `LOAD_WORK` and is deleted afterwards.
- `memcheck.py plateau -- CMD...` (or `--from SAMPLES`): one long run, sampled every 0.1 s. It passes when the peak after the first half of the run is at most `--flat-pct` above the peak before it, which separates bounded growth from a leak on 10M-doc runs.

Results go to `results/load/<run-id>/memcheck-*.json`; the L-802 summary is `results/local/L-802.json`. Set `MALLOC_ARENA_MAX=2` to compare with glibc limited to two malloc arenas; the files mode records the setting. On this box a 10M-doc run is limited to single cells. The full L1 tier at 10M docs needs a dedicated load host.

## Kill and resume checks (LOAD-006, `resume.py`)

`resume.py <tier> [--docs N] [--points 10] [--seed 1] [--only a,b,c,c1,d]` starts one engine (`es717-plain` by default), seeds `load-logs` and runs the README resume recipes verbatim, checking every result with `edverify diff`:

- `a`: a dump to a file is killed with `kill -9` at `--points` random places (picked from the staged `.NAME.part-PID-TS` file's size, since the tool prints no progress). The final file must be absent and the leftover must exist; the rerun with `--overwrite` must name the leftover in its warning and give the full digest.
- `b`: a restore (`--concurrency=8`) is killed with `kill -9` when the target's `_count` reaches a random point; rerunning the same command must give the same digest (writes go by `_id`).
- `c`: a restore is stopped with SIGINT or SIGTERM (concurrency 8 and 1) up to three times, each rerun using the printed `--offset N`; N must equal the previous offset plus that run's `Total Writes`.
- `c1`: after `kill -9`, the rerun uses `--offset` = the target's refreshed `_count` (README "count from what the cluster has", concurrency 1).
- `d`: `--scrollId` from your own first `_search?scroll=` page; first page + tool output must equal the index. It also records what a killed scroll dump loses (the pages it had fetched) when rerun with the same id.

Kill points are driven by the staged file size or `_count`, the `KILL_AT_DOCS` runner hook from the card. Results go to `results/load/<run-id>/resume.json`; the L-803 summary is `results/local/L-803.json`. L0 and L1L (1M) run on this box; 10M (L1) needs a dedicated load host. The resume recipe for cluster input on OpenSearch PIT depends on owner question Q4 (default sort) and is not covered here.

## Allocator comparison (L-802a, `allocbench.py`)

`allocbench.py --variant NAME=BIN[,ENV=VAL...] ... [--docs 100000,1000000] [--reps 3] [--run-id ID]` starts one engine (`es717-plain`, 2 GB heap), seeds a source index per size and runs restore, copy, dump and sliced dump (`--concurrency=8 --limit=5000`, `--slices=4`) for every variant, interleaved within each repetition. `BIN` may be `ldso:LIBDIR:BIN` to run a dynamically linked musl binary from the Alpine builder through `LIBDIR/ld-musl-*.so.1`. Per run it records docs/s, peak RSS and the memcheck plateau check (second-half peak at most 10% above the first-half peak). Build the variants with `cargo build --release` and `--features jemalloc` / `--features mimalloc`; environment variants such as `MALLOC_ARENA_MAX=2` or `MALLOC_MMAP_THRESHOLD_=131072` go after the binary. Results go to `results/load/<run-id>/allocbench.json`; the L-802a summary is `results/local/L-802a.md`.

Peak RSS is the child's own `VmHWM`, sampled every 0.1 s. It does not use `getrusage`/`wait4` `ru_maxrss`: Python starts children with vfork, so Linux counts the driver's own resident set in the child's maxrss (a `/bin/true` child of a 200 MB Python reports about 210 MB). `measure.py` (used by `run.py`/`matrix.sh`) and `memcheck.py` still use `ru_maxrss`, so a cell whose real peak is below the driver's own RSS reads as the driver's RSS (follow-up; not changed in L-802a).

## Fault injection under load (LOAD-007, L-804, `faults.py`)

`python3 scripts/load/faults.py <tier> [--docs N] [--only cut,storm,flood,restart] [--seed 1] [--run-id ID] [--keep]`
seeds `load-logs` on a fresh es717-plain node (data dir bind-mounted, so the node survives a restart), takes a
reference dump, then runs restore / copy / dump under each fault and verifies every exit-0 run with `edverify diff`
against the source. Pass rule (TASKS-LOCAL L-804): each run either completes with equal digests or exits non-zero
with a clear message; exit 0 with a digest mismatch (`SILENT_LOSS`), a panic or an empty stderr (`UNCLEAR_FAILURE`),
or a failed dump that leaves its final file is a failure. Non-zero exits are recorded for the lead to classify.

- `cut`: an in-script TCP proxy (counted and seeded, so no toxiproxy image is needed). `conn/50` resets every 50th
  connection at a random byte of its response (the literal spec; a run opens only ~1-15 pooled connections, so it
  rarely fires); `conn/4` makes it fire; `req/50` cuts every 50th request either before forwarding or after the
  server processed it (response dropped, RST). Scroll and PIT variants for copy and dump.
- `storm`: a second node started with `IT_ES_EXTRA_SETTINGS="thread_pool.write.size=1 thread_pool.write.queue_size=1"`
  (static settings, passed through `scripts/it/up.sh`); records the write-pool rejections.
- `flood`: at 20-40 % progress the watermarks go to 1 % and `index.blocks.read_only_allow_delete` is set on the target
  (what flood stage does, set directly so it lands at the chosen point), held 30 s, then lifted.
- `restart`: `podman restart -t 10` of the node at 20-60 %. Rootless podman + pasta on the box often fails the start
  half with "Address already in use"; the script then runs `podman start` until it succeeds (`start_retries`).

Output: `results/load/<run-id>/faults.json` and a `FAULTS-SUMMARY` line; containers and the work dir are removed
unless `--keep`. 10M with real flood-stage disk watermarks is Phase B (dedicated host).

## Compression levels (LOAD-008, L-805, `compress.py`)

`python3 scripts/load/compress.py <tier> [--docs N] [--reps 3] [--levels gzip:1,gzip:6,gzip:9,zstd:1,zstd:3,zstd:9,zstd:19]
[--part-size 64mb] [--slow-reps 1] [--run-id ID] [--keep]`

The script seeds `load-logs` and takes an uncompressed reference dump. Each rep then runs every codec and level, in
shuffled order with an uncompressed baseline, in two modes:

- **file to file** (`f2f`): no cluster, so the CPU-s above the baseline is the codec's cost.
- **cluster to file** (`dump`).

zstd levels of 19 and above run `--slow-reps` times, because they compress at about 2 MB/s per core.

Checks:

- **Decoding:** each output decodes with an independent decoder: gzip(1) for gzip, and for zstd the Python zstandard
  module from the venv at `ZSTD_PYTHON` (default `/workspace/tools/zvenv/bin/python`; create it with
  `python3 -m venv … && pip install zstandard`).
- **Read-back:** the tool reads each output back byte for byte, and dumps are digest-equal to the index.
- **Parts:** for `--fileSize` parts, `cat` of the parts decodes, and the tool reads it back.

Output goes to `results/load/<run-id>/compress.json`. Multi-GB single files at 10M are Phase B.

## Splitting at scale (LOAD-009, L-806, `split.py`)

`python3 scripts/load/split.py <tier> [--docs N] [--engine es717-plain] [--run-id ID] [--keep]`

The script seeds `load-logs`, takes a one-file reference dump, and then splits with `--fileSize` and `--maxRows`, from
the reference file (`file-*`) and from the cluster (`dump-*`). The cases are:

- The spec's literal values (`--fileSize 256mb`, `--maxRows 1000000`).
- Scaled values that give about 40 byte parts and 10 or 4 row parts at any `--docs`.
- gzip and zstd parts, and `--slices 4`.

Checks for each case:

- **Names and files:** part names are contiguous `p.split-0…N`, and there are no extra files.
- **No document spans parts:** every part ends with a newline, and every line is a whole JSON document with an `_id`.
- **Limit:** no part goes past the limit by more than its last line, and every part but the last reaches the limit.
- **Part count:** the expected count is `ceil(docs / maxRows)`, or a replay of the byte rule over the reference order.
  `--slices 4` is only bounded.
- **Digest:** an edverify diff of the index against all parts.
- **Staging (BRK-RES-002):** the output directory is polled every 20 ms. A visible part may appear only in the commit
  window, when staged + visible parts equal the final count.

Extras:

- Restore every part, one invocation per part, for a plain set and a zstd set, and diff the result with the source.
- `kill -9` mid-split leaves no visible part, and an `--overwrite` rerun is digest-equal.
- `--overwrite` with fewer parts replaces the set, leaves the older higher-numbered parts, and logs a warning.

Output goes to `results/load/<run-id>/split.json`. 10M docs with 256 MB parts is Phase B.

## Concurrency curves (LOAD-010, L-807, `curves.py`)

`python3 scripts/load/curves.py <tier> [--docs N] [--reps 3] [--points 1,2,4,8,12,16] [--shards 1,4]
[--baseline FILE] [--threshold 10] [--write-baseline FILE] [--run-id ID] [--keep]`

The script provisions one engine without a CPU cap and seeds `load-logs` (1 primary shard). For each extra
`--shards` value it makes a copy with `_split`, force-merged to one segment per shard. Each rep runs every point in
shuffled order:

- **dump:** `--slices S --workers S`, with scroll and with PIT (`--searchAfter --pit`), on each index.
- **restore:** `--concurrency C` from a one-file dump into a fresh 1-shard index (`--noRefresh`).

Each run records wall s, docs/s, the tool's CPU-s (wait4) and the engine's CPU-s (its process tree in `/proc`; this
includes background merges). Each point gets the median, min, max and CV over the reps.

Checks:

- Every run exits 0 with the full doc count.
- On the first rep, every dump and every restore is digest-equal to its source (edverify).
- **Regression gate:** `--baseline` fails on a drop of more than `--threshold`% in any point's median docs/s
  (TST-LOAD-010). The box baseline is in `baselines/box-L1L-es717-plain-curves.json`.

Output goes to `results/load/<run-id>/curves.json` and `curves.csv`. On the shared 8-vCPU box the curves are trends;
the authoritative 10M curves on dedicated cores are Phase B (B10).

## Node comparison (LOAD-011, L-808, `nodecmp.py`)

`python3 scripts/load/nodecmp.py <tier> [--docs N] [--reps 3] [--node PATH] [--scenarios a,b,...] [--run-id ID] [--keep]`

This is the load-scale version of `scripts/benchmark/quick-vs-node.py`. It runs the README Performance scenarios and
the PLAN.md §8 scenarios for upstream Node elasticdump and for searchdump, with the same flags for both. The Node
CLI defaults to `/tmp/ed/node_modules/.bin/elasticdump`, or `$NODE_ED`. It must be installed with
`--ignore-scripts`; the script only runs the CLI and never runs npm.

The script provisions one engine without a CPU cap and seeds `load-logs`. It splits that index into a 2-shard copy,
force-merges it and dumps a reference. Each rep runs these scenarios, in shuffled order and with the two tools in
shuffled order:

- **dump-defaults:** no flags (limit 100).
- **dump-5000:** `--limit 5000`.
- **dump-5000-gzip:** `--limit 5000 --fsCompress`.
- **dump-sliced:** Node `--limit 5000` against Rust `--slices 2` (= shards) and Rust `--searchAfter --pit --slices 8`.
- **restore:** the reference into a fresh 2-shard index, `--limit 5000 --concurrency 4 --noRefresh`.
- **copy:** index to index, with the same flags.

Each run records wall s, docs/s, CPU-s and max RSS (wait4), plus the scroll contexts it left open on the engine. The
summary gives the median, CV, the speed-up against Node and the Node/Rust CPU ratio.

Checks:

- Every run exits 0. Restore and copy reach the full count.
- On the first rep, every dump is digest-equal to the index. Each Node output is digest-equal to each Rust output for
  the same scenario. Restore and copy targets are digest-equal to the source.
- No Rust run leaves a scroll context open. Node does leave them open (one per shard until `scrollTime` expires);
  this is recorded, not failed.

Output goes to `results/load/<run-id>/nodecmp.json`. The ratios are reported against the §8 targets, not asserted:
on the shared box, one engine node next to both tools bounds restore and copy. Node at 10M on a quiet machine is
Phase B.

## Many indices, fields and aliases (LOAD-014, L-809, `manyidx.py`)

`python3 scripts/load/manyidx.py [--engine es717-plain] [--indices 300] [--docs 100] [--aliases 100] [--fields 5000] [--heap 2048m] [--run-id ID] [--keep]`

Creates `many-000`.. (edgen logs, 1 shard, 0 replicas), aliases over three indices each (some filtered or
routed) and one `wide-<fields>` index; dumps `many-*` as data, mapping, settings, index and alias, restores
twice (settings -> mapping -> alias -> data, then index -> data) into the host-only URL, checks every data set
with `edverify diff` and every re-dumped meta type against the first dump. Probes: a point-in-time dump of
`many-*`, and `--slices 2` (2 scroll contexts per shard, past ES's default `search.max_open_scroll_context`
500), which must fail at once with exit 1 and a hint. Meta types must finish within 300 s each. Run it with the
isolated podman store on PATH on the shared box. 1,000 indices is Phase B.
