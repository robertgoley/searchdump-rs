# Sequential dump/restore speed suite

**Status: defined and offline-tested; real Podman runs and initial baselines are pending.**
This is the single-machine entrypoint for repeatable performance regression tests and
comparisons with **Node elasticdump 6.125.1**, including `searchdump-multi` against Node's `multielasticdump`.
It complements, not replaces, the fault, memory, compression-level and concurrency
investigations in [../load/README.md](../load/README.md).

To measure a **published release** (or a local binary dir) on a Linux server,
with the same engines and seeders, and write results you can commit back from
this checkout, see [RELEASE-EVAL.md](RELEASE-EVAL.md).

## Internal names (kept for compatibility)

A few identifiers still use the pre-rename `edrs-` prefix. They are part of on-disk schemas and
paths; renaming them would break baseline identity checks and existing scripts, so they stay as is.

- `edrs-perf/v1` — protocol / schema id of this suite's `combined.json` (and the plan). A mismatch is
  treated as an incomparable baseline.
- `edrs-metrics/v1` — schema id written by `scripts/load/measure.py` / `measure.sh` for each timed
  command. The suite consumes those files.
- `edrs-odt-podman` — directory name of the isolated rootless Podman store used by the OpenDAL tests
  and the beta1 perf driver (`~/.local/share/edrs-odt-podman`, runroot `/tmp/edrs-odt-podman/run`), so
  they do not use the shared container store. The path is kept so existing runs keep working.

## Start here

```bash
# Print the entire plan offline: no image builds/pulls, sockets, engines or result writes.
scripts/perf/run-suite.sh --dry-run
scripts/perf/run-suite.sh --tier L1 --dry-run

# Real run: ALL profiles, sequentially; one verified warm-up + five measured reps.
scripts/perf/run-suite.sh
scripts/perf/run-suite.sh --engines es717-plain,os2-plain --cases dump-none,restore-none,copy --reps 3
scripts/perf/run-suite.sh --tier L1 --skip-node --baseline-dir results/perf/baselines
# Record a first, correctness-gated baseline (never silently replaces old failing results).
PERF_HOST_ID=ubuntu-dedicated scripts/perf/run-suite.sh --update-baselines
```

CLI:

```
run-suite.sh [--engines LIST] [--cases LIST] [--tier L0|L1] [--reps N]
             [--skip-node] [--baseline-dir DIR] [--update-baselines]
             [--threshold PCT] [--place PLACE] [--dry-run] [--host-tools]
```

**Host-tools mode** (`--host-tools` or `PERF_HOST_TOOLS=1`): no controller image is built or
run, so no apt/npm access is needed inside a container (use it when a proxy breaks the
Containerfile's `apt-get`). `suite.py` runs directly on the host (Python 3.11+, podman, bash,
curl, openssl, git). Unless `PERF_BIN_DIR` is set it builds the four release binaries with
`cargo +${PERF_TOOLCHAIN:-1.91.1} build --release` and uses `target/release`. `NODE_ED_BIN`
defaults to `/workspace/node-ed/node_modules/.bin` (Node elasticdump 6.125.1, installed with
`npm ci --ignore-scripts`). Baselines record `controller=local`. The container controller
stays the default.

```
scripts/perf/run-suite.sh --host-tools --tier L0 --reps 3 --update-baselines
```

Lists are comma-separated exact IDs. Unknown/duplicate IDs and nonpositive reps are errors.
Selection does not change engine order. `--baseline-dir` must be inside the repository,
which is mounted into the controller. Default: `scripts/perf/baselines/` (no invented or
imported measurements). `--place` defaults to `LOAD_PLACE` or `big` (dedicated host).
The plan's place/tier guards still apply; `big` does not bypass actual free RAM/disk checks.

### Host prerequisites / container builds

Normal runs need **rootless Podman, Bash and ordinary Linux core utilities only**: no host
Rust, Python, Node/npm, sudo, Docker, private registry login or cloud resources.
Offline `--dry-run`/`--help` additionally need host Python **3.11+**, but do not need Podman.
`python3 scripts/perf/selftest.py` runs the offline tests.

1. Pre-pull all **11 exact `image:tag@digest` entries** from
   [../it/images.lock](../it/images.lock), including Alpine/toxiproxy helpers (which are
   checked but not started as database engines). Missing pins are a hard prerequisite
   error, not a silently skipped engine. `../it/check-pins.sh` remains authoritative.
2. The entrypoint builds `localhost/searchdump-rs-perf:1` if absent, using
   `Containerfile`: the existing public Rust/Debian builder digest, Python, curl,
   OpenSSL, a Podman remote client, Node/npm, and Rust **1.91.1**. npm uses the checked-in
   integrity-locked dependency graph, exact elasticdump 6.125.1, and `--ignore-scripts`.
   Lifecycle scripts are never executed. The apt-provided Node/runtime packages are not
   digest-pinned independently: retain the built controller image for comparable runs.
3. Release `searchdump`, `searchdump-multi`, `edgen`, `edverify` are built **inside
   that controller**, before starting any engine, with
   `cargo +1.91.1 build --locked --release --workspace --bins`. The persistent target is
   `results/perf/build`; Cargo runs on every invocation to avoid stale executables.
4. The controller uses a **private, temporary host Podman API socket**, starting sibling
   containers in the host's existing rootless image store, not nested Podman storage.
   Socket access confers the same control as your Podman CLI; run only trusted suite code.
   The socket/service are removed on exit. The repo is mounted at the identical absolute
   path so remote bind mounts resolve correctly. The controller uses `--network=host`;
   **it never combines host networking with `--userns=keep-id`**. Image builds and Cargo/npm
   network steps consequently use the host resolver, avoiding Ubuntu's default-network DNS issue.
   Engines keep the existing loopback-only published port (19222, or `PERF_ENGINE_PORT`); they need no external DNS.
   If that port is already taken (e.g. a leftover engine), each engine fails fast with a clear message
   instead of a podman/pasta bind error on every case.

Optional env variables (no credentials belong in this repository):

- `PERF_CONTROLLER_IMAGE`: prepared controller image; remove/rebuild the default tag after
  Containerfile/dependency changes. Its actual image ID is recorded in baseline identity.
- `PERF_BIN_DIR`: absolute directory containing all **four release** executables. Mounted
  read-only; skips Cargo builds. Use a libc-compatible build; binary SHA-256s are recorded.
- `NODE_ED_BIN`: absolute local npm `node_modules/.bin` directory containing both Node CLIs.
  Its parent is mounted read-only so npm symlinks resolve. A runnable local installation
  at any other layout must put the symlink targets under that parent too.
  Version must report exactly 6.125.1. Otherwise the controller's locked install is used.
- `PERF_BUILD_JOBS`: Cargo jobs, default 2. `PERF_HOST_ID`: stable machine/configuration name
  (default host hostname); change it for CPU, storage or power-policy changes.
- `LOAD_PLACE`: existing `scenarios/tiers.toml` place. Do not use shared-host runs to make
  authoritative speed claims. Absolute custom `LOAD_WORK` is not used: payloads live under
  the suite result directory so all remote mount paths are consistent.

Builds/pulls, readiness, seeding, reference creation, refreshes, verification and teardown
are **outside** measured intervals. Each measured interval includes CLI/process startup,
transport, file close and any child shutdown. OS caches are deliberately warm; there is
no privileged cache-dropping, forced cold-cache claim or CPU governor change.

## Engine sequence and capability exclusions

For each row: **plain, teardown, secure, teardown**, then the next row. Every selected
profile runs in a fresh engine. These cover **all nine database images** in the 11-image
lock (ES 7.10 plain and secure intentionally use different distributions).

| Sequence | Stable profile prefix | Pinned engine |
|---|---|---|
| 1 | `es710` | ES 7.10.2: bitnamilegacy OSS plain / official Elastic secure |
| 2 | `es717` | official ES 7.17.29 |
| 3 | `elasticsearch8` | bitnamilegacy ES 8.6.1 |
| 4 | `es819` | official ES 8.19.23 |
| 5 | `elasticsearch` | bitnamilegacy ES 9.1.2 |
| 6 | `os13` | OpenSearch 1.3.20 |
| 7 | `os2` | pinned OpenSearch `2` (currently 2.19.7) |
| 8 | `os3` | pinned OpenSearch `3` (currently 3.9.0) |

Full IDs are e.g. `os3-secure`, **not** `OS_TAG=3 opensearch-secure`. `matrix.sh` maps
profiles to `up.sh`; mutable ambient image/tag overrides are removed and lock digests
are enforced. Runtime engine versions and exact images are retained in results.

- `dump-pit-*` skips **both ES 7.10 distributions and OS 1.3** conservatively. The official
  ES 7.10 distribution has PIT, but the OSS one does not; this suite does not make a
  capability claim for the official distribution. All newer profiles run PIT cases.
- Node has no equivalent slicing, PIT or zstd cell: those cells are **Rust-only**, never
  presented as an equal-flags Node speed comparison. There is no silently unsliced Node
  number alongside a sliced Rust number. Node multi split loads are broken in 6.125.1,
  so the multi scaling cells use unsplit/plain data only.
- Plain/secure results and baselines are separate. Secure profiles use the harness's
  ephemeral basic-auth credentials. They are passed via env / a private Node auth file,
  not command-line credential values or committed configuration. To handle the demo
  OpenSearch certificates and Rust multi parent's CA limitation uniformly, **all secure
  timing cells disable certificate verification for both tools** (including Node discovery).
  This measures TLS/auth overhead, not PKI verification; verified-CA security tests remain
  in the integration suite. Never use this protocol against production endpoints.
- A Node runtime/API failure on a newer engine is a **reported failure**, not an automatic
  compatibility skip or a flattering Rust-only ratio. Failed results cannot update baselines.

## Data sets and resources

Generators, mappings, accepted-document seeding and reference semantics are reused from
`load/run.py` and the existing `dump-restore-logs.toml` / `memcheck.toml` profiles. Seed=1.
No new downloaded corpus, Rally credentials or synthetic substitute for existing samples.

| Dataset | Generator | L0 | L1 | Page/bulk `--limit` (both tools) |
|---|---|---:|---:|---:|
| logs | `logs`, ~1 KB/doc | 100,000 docs | 10,000,000 docs | 2000 |
| small | `geo`, ~300 B/doc | 100,000 | 10,000,000 | 2000 |
| big | `huge`, generator default 1–5 MiB/doc | 33 | 3333 | **10** (avoids multi-GB bulk buffers) |
| edge | `edge`, prior round-trip corpus | 10,000 generated | 10,000 generated | 2000 |
| many | `logs`, 16 indices, same total count | 6250/index | 625000/index | 2000 |

The edge generator intentionally contains duplicate/rejected cases. Counts/reasons and
actual stored counts are retained; the reference is what the engine accepted, never the
nominal generated count. Any rejection/overwrite on non-edge profiles fails seeding.
Generators' real serialized byte sizes are recorded; size labels are estimates.

Reuse `tiers.toml` heap: **L0 1024 MB; L1 4096 MB** (place caps are marked in plan). OpenSearch 2.16+ ships query-insights with top_queries on by default; `scripts/it/up.sh` disables those collectors for IT/perf. OS 2/3 still use a **2048 MB** L0 heap so restore-big (source+dest huge docs resident) does not trip the parent circuit breaker; other engines keep the tier 1024 MB heap.
Small nodes are not covered here: the smallest heap in this suite is 768 MB (place `shared`). The low-memory set for `searchdump-multi` on 256 MB and 384 MB heaps (1.1-10) is `scripts/load/lowheap.py`; see [scripts/load/README.md](../load/README.md#low-memory-engines-load-lm-l-818-lowheappy). Run it before a release next to this suite.
Only **one dataset** is resident at a time, with at most source + restore target + two
file copies. The suite reserves `6 × tier_docs × 1000 bytes + 1024 MB` disk (L0 ~1.6 GB,
L1 ~61 GB, excluding pre-pulled images/build cache), checks it against the place budget
and actual free work disk, and requires `2×heap + 512 MB + tool_mem_mb + 256 MB` free RAM.
With default place `big`, that is about **4.8 GB / 11 GB** free RAM. It also gates measured
max RSS against the existing place tool budget. Big docs use proportionately fewer
records, not ten million multi-MB documents. For L1 use a dedicated host with room for
all pinned images, build products, and engine storage in addition to the payload estimate.
Podman storage may be on another filesystem: ensure it has the same free-space allowance.

The controller holds the **same host `/tmp/searchdump-it.lock`** as IT/load runners for
the entire sequence. Existing IT containers cause refusal without deleting them. Container
RAM/CPU limits come from `up.sh` (CPU count is the host's available count for both tools);
rootless systems must have cgroup-v2 delegation to enforce Podman caps. No claimed watchdog
or affinity pinning beyond these limits. Keep other host work quiet and record changes
under a new host ID. Before the next engine, `down.sh` is scoped by RUN_ID and removes
only this run's containers/volumes. Named Bitnami data volumes allow the remote client
path without unsupported remote `podman unshare`. Ordinary IT bind mounts remain the default.
A test/engine failure continues to the next engine after teardown. **Teardown failure
stops the sequence** (remaining cells become NOT_RUN), rather than risking two engines.

## Exact case catalogue

Each row is expanded into a warm-up and `--reps` measured runs per supported tool. Common
single-file flags: `--type=data --concurrency=1` and dataset limit above. Restore/copy
use a fresh one-shard, zero-replica target with the source mapping; target creation and
post-run refresh are outside timing. `copy` is HTTP cluster→cluster **within the same
single-node engine**, different source/target indices; this deliberately isolates tools
without violating the one-engine invariant. Remote/cross-engine network benchmarking is
still a separate pending test in TESTING-TODO.

Plain means no compression flag. gzip means `--fsCompress --compressionLevel=6`; zstd
means `--compressionType=zstd --compressionLevel=3`. `--fileSize=20mb` applies to split
dumps. Split restores invoke each numerically sorted part sequentially through the same
small `parts.py` wrapper for both tools, with the wrapper/process-start costs included.
Restore inputs are common **Rust-created, edverify-checked files**, not whichever tool's
previous timed output happened to be fastest. Each input codec has its own verified
reference. Metadata restore references are checked by the post-restore metadata gate.

| ID | Dataset | Measurement / flags | Tools |
|---|---|---|---|
| `dump-none` | logs | cluster → file; none | Rust + Node |
| `restore-none` | logs | file → fresh index; none | Rust + Node |
| `dump-gzip` | logs | cluster → file; gzip | Rust + Node |
| `restore-gzip` | logs | file → fresh index; gzip | Rust + Node |
| `dump-zstd` | logs | cluster → file; zstd | Rust only |
| `restore-zstd` | logs | file → fresh index; zstd | Rust only |
| `copy` | logs | HTTP index → fresh index; none | Rust + Node |
| `dump-scroll-s1` | logs | cluster → file; `--slices=1` | Rust only |
| `dump-pit-s1` | logs | cluster → file; `--searchAfter` `--pit` `--slices=1` | Rust only |
| `dump-scroll-s2` | logs | cluster → file; `--slices=2` | Rust only |
| `dump-pit-s2` | logs | cluster → file; `--searchAfter` `--pit` `--slices=2` | Rust only |
| `dump-scroll-s4` | logs | cluster → file; `--slices=4` | Rust only |
| `dump-pit-s4` | logs | cluster → file; `--searchAfter` `--pit` `--slices=4` | Rust only |
| `dump-scroll-s8` | logs | cluster → file; `--slices=8` | Rust only |
| `dump-pit-s8` | logs | cluster → file; `--searchAfter` `--pit` `--slices=8` | Rust only |
| `dump-split-none` | logs | cluster → file; split none (20 MB parts) | Rust + Node |
| `restore-split-none` | logs | file → fresh index; split none (20 MB parts) | Rust + Node |
| `dump-split-gzip` | logs | cluster → file; split gzip (20 MB parts) | Rust + Node |
| `restore-split-gzip` | logs | file → fresh index; split gzip (20 MB parts) | Rust + Node |
| `dump-split-zstd` | logs | cluster → file; split zstd (20 MB parts) | Rust only |
| `restore-split-zstd` | logs | file → fresh index; split zstd (20 MB parts) | Rust only |
| `dump-small` | small | cluster → file; none | Rust + Node |
| `restore-small` | small | file → fresh index; none | Rust + Node |
| `dump-big` | big | cluster → file; none | Rust + Node |
| `restore-big` | big | file → fresh index; none | Rust + Node |
| `dump-edge` | edge | cluster → file; none | Rust + Node |
| `restore-edge` | edge | file → fresh index; none | Rust + Node |
| `mapping-dump` | logs | mapping-only dump | Rust + Node |
| `mapping-restore` | logs | mapping-only restore | Rust + Node |
| `settings-dump` | logs | settings-only dump | Rust + Node |
| `settings-restore` | logs | settings-only restore | Rust + Node |
| `multi-dump-p1` | many | 16-index directory dump, `--parallel=1` | Rust + Node |
| `multi-restore-p1` | many | 16-index directory load, `--parallel=1` | Rust + Node |
| `multi-dump-p4` | many | 16-index directory dump, `--parallel=4` | Rust + Node |
| `multi-restore-p4` | many | 16-index directory load, `--parallel=4` | Rust + Node |
| `multi-dump-p8` | many | 16-index directory dump, `--parallel=8` | Rust + Node |
| `multi-restore-p8` | many | 16-index directory load, `--parallel=8` | Rust + Node |

Multi cases time each whole parent's run and all its children. Both get
`--includeType=data --interval=0 --parallel=P --limit=2000 --concurrency=1`.
Rust additionally uses `--nodeCompat --compression=none --noDefaultDataFlags`, placing
child-only flags after `--`; Node's parent receives them directly. Thus both write
`INDEX.json`, unsplit, without metadata transfer. The source reference directory has
nonempty mapping markers for Rust's safe manifest-free discovery (Node ignores those
when data-only is selected). Both loads auto-create fresh prefixed target indices;
pre-creation would make Rust searchdump-multi refuse an existing index. Every restored
index is compared to its own source, so dynamic mapping problems fail the cell.
Rust manifest/child logs are kept out of the payload-byte measurements. Parallelism is
index-level **1, 4, 8**, not an unequally configured per-document writer.

## Repetition, correctness, metrics, reporting

Per engine/dataset/case:

1. Seed with existing `seed_index`, make a plain reference, and **edverify diff** against
   the stored source before accepting any result. Sources are immutable thereafter.
2. Run one uncounted warm-up for each supported variant. Warm-ups must also be correct.
3. Run N repetitions (default **5**), rotating/shuffling the paired Rust/Node order with
   a fixed seed. Cases follow catalogue order within datasets. Warm caches are part of
   this protocol; do not mix these numbers with cold-cache experiments.
4. `measure.sh` wraps each invocation; schema-validation uses the existing
   `measure.py`/`metrics.schema.json`. Every sample must exit 0, have no failed writes,
   respect RSS budget and pass **edverify diff --canonical --index-as x** before its
   timing counts. Node samples add `--decimal-numbers`: Node's JSON.parse/stringify
   respells numbers (`79.746790` -> `79.74679`, `1.0` -> `1`) without changing their value,
   so Node is compared by exact decimal value; a changed value (rounded integers above
   2^53, shortened mantissas, `-0` -> `0`) still fails. Both dump files (all split parts) and every restore/copy target are
   verified **on every repetition**, not just the first one. Total Writes, when the
   wrapper captures it unambiguously, must match the accepted source count.
5. Metadata-only cells round-trip the **actual timed dump** or restore the reference,
   then wrap normalized source/target metadata as canonical `_source` records for
   edverify. Mappings must match; settings exclude only generated UUID, creation date,
   provided name, version and history UUID. They report operations/s, not document throughput.
   Reference creation and the untimed half of the metadata round-trip are not included.
6. Any failed Rust warm-up/repetition invalidates that entire case's aggregate for **both**
   variants. A failed **Node** sample is the comparison's problem, not the product's: the
   case keeps its Rust result and is marked `NODE_DIFF` (Node's output differs in value,
   e.g. the `edge` numeric cases) or `NODE_FAIL` (Node errored or broke the RSS budget), and
   Node is not run again for that case. Neither blocks `--update-baselines`.
   Every timed command and verification has a wall-clock limit (`PERF_CMD_TIMEOUT_S`,
   default 1800 s); on expiry its whole process group is killed. For Node that is
   `NODE_TIMEOUT`; for Rust it is a failure of the case. Rust must leave no scroll/PIT contexts. Node contexts are recorded, then
   explicitly cleared outside timing on this private engine to avoid polluting later runs.

Retain raw metrics, commands (existing redaction), stdout/stderr, verify reports, seed
acceptance details, engine version/digest, host ID, CPU count/kernel, git SHA, binary
hashes, controller image ID and Node runtime version. All live results are git-ignored:
`results/perf/<local-timestamp>-<pid>/ENGINE/`; combined report is `combined.json`
(schema `edrs-perf/v1`; see [Internal names](#internal-names-kept-for-compatibility)) plus `summary.md`. Private work/auth/PKI/payload files are removed
at dataset/engine teardown, while diagnostics and child logs are retained.

Each supported cell reports **median**, **nearest-rank p90**, **min/max** and **CV%** wall
time across counted runs. The JSON also includes medians for docs/s, physical input/output
MB/s, user/system/total CPU-s, max RSS, FDs and compression ratio (plain NDJSON bytes /
compressed file bytes). Logical MB/s = verified reference NDJSON bytes / wall seconds is
also recorded for data operations, including copy where physical file MB/s is inapplicable.
Metadata rates count one operation, not source documents. Compression ratio for multi
refers to data payloads, not Rust's manifest. Full raw metrics retain sampled RSS,
threads, I/O, context switches, retries and cluster stats.

Caveat inherited from `measure.py`: wait4 max RSS is a process/descendant high-water value,
**not the sum of simultaneous searchdump-multi workers**, and can include the launcher's
RSS floor; `/proc` sampling includes the parent and immediate children, not every
recursive descendant. CPU/RSS multi figures are therefore diagnostic, not authoritative
whole-tree accounting. Wall throughput remains comparable because it includes all work.
The suite does not mislabel Podman-client CPU as tool CPU: measured tools run **inside
the controller**, not through one `podman run` per timed command.

### Regression and Node rules

- Baselines are per **host ID + place + tier + engine/secure variant**, with identity
  checks for suite protocol, exact engine digest, heap, limit, dataset sizes and controller
  image ID, full case manifest and generator source hash. A mismatch is a failure requiring an explicit new baseline location/identity,
  not permission to overwrite incomparable measurements. Hardware/OS/storage changes
  require a new `PERF_HOST_ID`. Repetition counts and all selected flags remain in the plan.
- `load/compare.py BASE NEW --threshold 10` compares **Rust median metrics**: wall/CPU/
  max RSS/FD increases or docs/s / physical MB/s decreases beyond **10%** fail with
  **REGRESSION** and suite exit **1**. The markdown baseline delta is the **wall** delta;
  a CPU/RSS regression can fail even when that wall delta is good. Raw comparator output
  names the actual metric. Equality at the threshold passes. `--threshold` changes it.
- Missing baselines report **PASS (NO_BASELINE)**, explicitly *not regression-tested*.
  There are no seeded speed claims. Existing `load/baselines/*.json` have different
  scenario/measurement protocols and are intentionally not misused as this suite's baselines.
- Node ratio = **Rust median wall / Node median wall**, lower is better. Values above
  1 get JSON `node_comparison=SLOWER`; ratios are **informational warnings**, never a
  performance pass/fail gate by themselves. Unsupported Node cells have null ratio and
  explicit skip reasons. There is no performance claim from a failed correctness gate.
- `--update-baselines` writes/merges only cells from an entirely correct,
  regression-free engine run. Failed, mismatched or refused engines never overwrite a
  baseline; partial selections preserve other cells. To deliberately approve a known
  regression, use a new baseline directory after reviewing it, rather than weakening
  correctness. Baselines can be committed separately after reviewing real measurements.
- Exit **0**: all selected supported cases correct and no baseline regressions;
  **1**: a case/engine/resource failure or regression; **2**: invalid configuration,
  missing prerequisites or shared-lock contention. Capability skips alone do not fail.

## What remains to validate on a Podman host

Run L0 across all 16 profiles with Node enabled, investigate compatibility failures,
then record dedicated-host baselines. Run L1 with sufficient RAM/disk, repeat against
those baselines, and confirm p90/spread stability. Validate controller build/remote API,
Bitnami named-volume permissions, secure auth/discovery, all engine PIT paths, codec and
split restores, teardown after injected failure/interruption, and localhost-only ports.
Offline self-tests are not substitutes for any of those measurements. No real-engine
result is claimed by adding this suite.
