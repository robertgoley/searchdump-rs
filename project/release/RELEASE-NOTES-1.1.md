# searchdump-rs 1.1.0 release notes

Status: version `1.1.0` (`Cargo.toml`), tagged `v1.1.0`. The 16-engine L0 RC perf results (575 PASS, 1 known
FAIL: ES 8.6 plain multi-restore-p8, see `project/plan/REVIEW-1.3.md` 1.3-R1) were added after the `v1.1.0` tag, on
`release/1.1` (`results/local/1.1-rc.md`); no code changed.

The same material is summarised in [`CHANGELOG.md`](../../CHANGELOG.md) under **1.1.0**.
Known, deliberate differences from Node elasticdump are also in
[`NODE-DIFFERENCES.md`](../../NODE-DIFFERENCES.md) (#25–#27).

## Disclaimer

searchdump-rs is provided **as is, without warranty of any kind**, as set out in its
[MIT license](../../LICENSE) (the same warranty disclaimer as the elasticdump-rs code it derives
from; Node elasticdump, whose behaviour it follows, has the equivalent disclaimer in Apache-2.0,
sections 7 and 8). The authors accept no liability for any damage from its use, including data
loss, data corruption, downtime or service disruption, in production or anywhere else.

It has been stable and fast in our testing, but no test suite can cover every edge case across
cluster versions, configurations, plugins and storage backends. Before you rely on it:

- try it in a non-production environment first;
- verify every backup independently: restore it somewhere, then compare document counts (and
  spot-check documents, mappings and settings) against the source;
- keep your existing backup method until those checks pass for your own data.

See [Disclaimer](../../README.md#disclaimer).

## Highlights

- **Restores and copies spend less time refreshing.** The writer refreshes at most every 5 seconds
  (plus once at the end of the run) instead of after every bulk batch. With no output index, it
  refreshes only the indices it wrote, never the whole cluster.
- **Scroll `--slices` is capped to the shard count**, so a dump no longer asks the engine to filter
  every document of a shard once per extra slice. `--sliceField` can slice on a numeric or date
  field instead of `_id`.
- **`--scrollDocSort` is available as an opt-in** when you want Elasticsearch's cheap `_doc` scroll
  order. It stays off by default so dump lines still match Node and 1.0.
- **gzip and zstd no longer stall the writer** between batches. The compressed bytes stay identical
  to 1.0.
- **At `--concurrency 1`, the next bulk batch is prepared while one is sending.** Sends never
  overlap, so the cluster still sees the same requests in the same order.
- **`searchdump-multi` takes record counts from its children**, so a large compressed dump is no
  longer read back just to fill the manifest. Ctrl-C now stops running children. `--verifyCounts`
  keeps the old full re-read when you want it.
- **`searchdump-multi` no longer takes down small-heap clusters** (1.1-10). Data runs page 1000
  documents (fewer for large documents) with a 256 MB response cap, instead of 5000 with no cap,
  which killed 256 MB-heap Elasticsearch 7.10 and 8.6 nodes and tripped the 8.19 breaker.

- **Today's 1.0-line fixes are included** (merged 2026-11-14): exit status 4 for transient
  failures that are safe to rerun, `--httpAuthFile` winning over every other credential source,
  `--restartOnContextLoss`, the OpenSSH client for `sftp://` in the container images, about half
  the restore CPU, and clearer errors for the open scroll limit, HTTP 413 and routed documents
  sent to a data stream.

## New flags and defaults

| Flag | Default | Tool | Meaning |
|---|---|---|---|
| `--refreshInterval=<seconds>` | `5` | `searchdump` | Refresh written indices at most this often while writing, plus once at the end of the run. Accepts seconds, optional decimals, and an optional trailing `s` (`0` to `86400`). `0` refreshes after every batch, as in 1.0 and Node. |
| `--noRefresh` | off (unchanged) | `searchdump` | Sends no refresh. Wins over `--refreshInterval`. |
| `--scrollDocSort` | off | `searchdump` | Opt-in. Adds `"sort":["_doc"]` to an unscored scroll body that has no user sort. |
| `--sliceCap=auto\|min\|total\|false` | `auto` | `searchdump` | A scroll dump lowers `--slices` to the primary-shard count of the input indices (one shard: unsliced) and prints a warning. `auto` (`true` is an alias) caps only on Elasticsearch 7.10, to the smallest primary-shard count, and keeps `--slices` as given on every other engine and undetectable versions (1.1-4a). `min` / `total` force one; `false` keeps the number you passed. |
| `--sliceField=<field>` | unset | `searchdump` | Slice on this numeric or date field with doc values, instead of `_id`. The mapping is checked first; a missing, non-numeric, or `doc_values:false` field is an error before the first search. Using this flag turns the shard-count cap off. |
| data-run `--limit` / `--maxResponseMb` / `--maxBulkBytes` | `1000` (fewer for documents over 16 KB stored) / searchdump default 256 / `16mb` | `searchdump-multi` | 1.1-10: was `--limit=5000 --maxResponseMb=0` (and 100 MB bulks), which killed or overloaded 256 MB-heap Elasticsearch 7.10/8.6/8.19 nodes. `-- --limit=N` sets the page size and turns the per-index sizing off. |
| `--verifyCounts` | off | `searchdump-multi` | Always re-read dumped or loaded data and fail the index when the child's reported record count differs. |
| `--restartOnContextLoss N` | `0` (off) | `searchdump` | Restart a scroll data dump into a file from the start, up to N times, when the scroll context is lost. Not for PIT/search_after, `--scrollId`, or outputs other than a file. |

Related switches that are not CLI flags:

- `SEARCHDUMP_BULK_PIPELINE=0` turns off encode/send overlap at `--concurrency 1` (for A/B checks).
- `SEARCHDUMP_MULTI_DEBUG=1` prints how `searchdump-multi` got each record count.
- `--statsFile` is a hidden `searchdump` flag. `searchdump-multi` passes it only to a child whose
  binary resolves to `searchdump*` (the `elasticdump` / `elasticdump-rs` compatibility symlinks
  included). A Node `--elasticdumpBin` never receives it.

## Behaviour changes versus 1.0

These compare 1.1 with the 1.0 line, including 1.0.0-beta1. Dump files, the
`multielasticdump.json` manifest, environment variable names, and the compatibility symlinks are
unchanged.

**From the 1.0 line (merged into 1.1 on 2026-11-14).** These also land on `main` after 1.0.0:

- **Exit status 4** means a transient failure (network, HTTP 429/5xx, lost scroll context) that is
  safe to rerun once retries are spent. A restore that already sent documents without `_id`
  still exits 1, because a rerun would index them again.
- **`--httpAuthFile` wins** over URL credentials, `--input-*`/`--output-*`/`--username`/`--password`
  and `SEARCHDUMP_*` variables, with a warning that names (never shows) what it replaced.
- **`sftp://` works in the container images** that have a package manager (OpenSSH client added);
  `dhi-static` still has no `ssh`. The Debian images carry no user-admin or init-script tools
  (L-810j).
- **Clearer errors:** the open scroll limit fails at once with exit 1 and names
  `search.max_open_scroll_context`; HTTP 413 names `http.max_content_length` and `--maxBulkBytes`;
  routed documents into a data stream get one warning naming `data_stream.allow_custom_routing`;
  the final HTTP 429 names the retry and load flags; a mostly-blocked restore names the block.

- **Refresh after every batch is no longer the default.** 1.0 refreshed the target after every
  successful bulk batch. 1.1 refreshes at most every 5 seconds (the clock starts at the first
  finished batch) and always once at the end of the run, including the `--concurrency 1` path,
  which previously relied on the last batch's refresh. After the run, the same documents are
  searchable; a reader watching *during* a restore may see new documents up to the interval later
  (the engine's own `index.refresh_interval`, 1 s by default, still applies as well). Use
  `--refreshInterval=0` for the 1.0 request pattern, or `--noRefresh` to send none.
- **A restore with no output index no longer refreshes the whole cluster.** 1.0 sent
  `POST /_refresh` in that case, which refreshes every index. 1.1 sends
  `POST /a,b,…/_refresh?ignore_unavailable=true&allow_no_indices=true` for the indices it actually
  wrote (after `--prefix` / `--suffix` / rename; data-stream creates record the stream), split so
  each URL path stays under ~3 KB, and sends nothing when nothing was written. With an output index
  the request is unchanged from 1.0.
- **`--slices` on a scroll dump is capped to the shard count on Elasticsearch 7.10.** In 1.0,
  `--slices` was used as given, even when it was higher than the primary-shard count, which made
  ES 7.10 slower than an unsliced scroll. On ES 7.10, 1.1 lowers it to the smallest primary-shard
  count of the input indices (with a warning) unless you pass `--sliceCap=false`. **Every other
  engine, and an undetectable version, keeps `--slices` as given** (1.1-4a, owner decision
  2026-11-14): the point-in-time perf run showed the cap making sliced scrolls 45–59% slower on
  ES 8.19 and 62–101% slower on OpenSearch 3 (see Performance below). `--sliceCap=min|total`
  still force the cap on any engine (Elasticsearch 7.11–9.4 spread slices across all shards of
  the request, so `total` matches them; the others slice per index). When
  settings cannot be read (for example no `view_index_metadata` privilege), the requested number is
  kept and a warning is printed. PIT (`--searchAfter --pit`) is never capped.
- **`--scrollDocSort` does not change default dumps.** Leave it off and the scroll request and the
  dump lines are the same as 1.0. With the flag on, unscored scroll hits are written with
  `"_score": null` and without the returned `sort` member; restores ignore both. The flag is
  skipped, with an info log, for PIT, `--searchAfter`, `--scrollId`, `--searchWithTemplate`, a user
  `sort`, scoring queries, and `track_scores` / `min_score` / `rescore`.
- **Compressed file output is produced on a per-part thread.** There is no flag. gzip and zstd
  bytes are identical to 1.0. The memory estimate counts three extra batches for a compressed file
  output.
- **`--concurrency 1` prepares the next bulk batch while one is in flight.** Sends still do not
  overlap. An error in batch N means batch N+1 is never sent; a run that stops with an error still
  finishes the one batch in flight. The version-conflict (409) and data-stream hints are printed
  once per run at `--concurrency N > 1` as well (1.0 printed them once per batch there). The memory
  estimate counts two extra batches.
- **`searchdump-multi` Ctrl-C stops running children.** 1.0 left them to finish unless the terminal
  also signalled them. 1.1 starts no new step, sends SIGTERM, then SIGKILL after 5 seconds. Each
  stopped child is named with its log, and its index is recorded as failed. Exit status 130 is
  unchanged. A restore still finishes in-flight bulk requests on SIGTERM and prints where to resume.
- **`searchdump-multi` no longer always re-reads data files for counts.** It prefers the child's
  `--statsFile` JSON, then the last `Total Writes: N` (and `Failed Writes: N`) log line, and only
  then scans the files. Summary and manifest formats are unchanged. Pass `--verifyCounts` to keep
  the 1.0 always-rescan check and fail the index on a mismatch.
- **The perf harness timer is exact.** `scripts/load/measure.py` no longer rounds wall time up to
  the next ~0.1 s tick. The `searchdump` and `searchdump-multi` binaries are unchanged by that
  commit.

## Behaviour changes versus Node elasticdump

File formats stay compatible: dumps made by either tool still load with the other, and
`searchdump-multi --nodeCompat` is unchanged.

Deliberate differences introduced in 1.1:

- **Refresh cadence (NODE-DIFFERENCES #26).** Node refreshes after every bulk batch
  (`_data.js` `reindex()`). 1.1 refreshes at most every `--refreshInterval` seconds (default 5)
  plus once at the end. `--refreshInterval=0` restores Node's per-batch refresh.
- **Refresh targets (NODE-DIFFERENCES #25).** When there is no output index, Node posts
  `${base.url}/_refresh`, which refreshes **every index in the cluster** after every batch. 1.1
  refreshes only the indices it wrote. The data visible after the run is the same; unrelated
  indices are no longer refreshed. With an output index, both tools refresh that index.
- **Scroll `_doc` sort (NODE-DIFFERENCES #27).** Node sets `sort: ['_doc']` on its request
  *options* object, not the body, so it never sends a sort and writes `"_score": 1`. 1.1 matches
  that by default (no sort, identical dump lines). Opt-in `--scrollDocSort` actually sends
  `"sort":["_doc"]` and writes `"_score": null`.

No Node compatibility difference (searchdump-only behaviour, or the same requests as Node):

- `--slices`, `--sliceCap`, and `--sliceField` (Node has no slicing).
- Off-thread compression (gzip bytes identical; zstd is a searchdump extension).
- Encode/send overlap (same requests in the same order at `--concurrency 1`).
- `searchdump-multi` Ctrl-C handling (not part of the dump format or CLI contract).
- Child-reported counts (`--statsFile` is never passed to a Node binary; Node's
  `Total Writes: N` log line is still parsed).

## Upgrade guide

1.1 is the next stable line. These changes are not backported to 1.0.

- **If a script checks for exit status 1 exactly**, also accept 4: it marks a transient failure
  that is safe to rerun.
- **If you pass `--httpAuthFile` together with other credentials**, the file now wins.

- **Existing dumps keep working.** NDJSON data files, metadata files, split parts, and
  `multielasticdump.json` are unchanged. `SEARCHDUMP_*` names, the deprecated `ELASTICDUMP_*`
  aliases, and the `elasticdump` / `elasticdump-rs` / `multielasticdump` compatibility symlinks
  are unchanged.
- **If a script or watcher needs documents to become searchable after every batch**, pass
  `--refreshInterval=0` (Node-identical requests) or `--noRefresh` if you do not want a refresh
  at all. Otherwise the new 5-second default is the one you get, including from
  `searchdump-multi` data runs unless you pass another value after `--`.
- **If you restore many indices into their original names** (no `--output-index`), only those
  indices are refreshed. That is intentional. Unrelated indices on the same cluster are no longer
  touched.
- **If you pass `--slices` higher than the primary-shard count on ES 7.10** and want that number
  kept, add `--sliceCap=false`. Other engines keep it without a flag (1.1-4a). To force a limit
  on any engine, use `--sliceCap=min` or `--sliceCap=total`. Live tests that exercise over-slicing already do this. PIT is not capped.
- **Leave `--scrollDocSort` off** when you compare dump files with Node or with 1.0, or when a
  scoring query / user sort / PIT / search-after is in use (the flag already refuses those
  cases). Turn it on only when you want `_doc` order on a plain scroll dump.
- **Ctrl-C on `searchdump-multi` now stops children.** Do not rely on them running to completion
  after the parent is interrupted. Use `--resume` (and the child's log) to continue a stopped
  load.
- **`searchdump-multi` data runs use smaller pages** (1.1-10): 1000 documents (fewer when the
  documents average more than 16 KB stored) and a 256 MB response cap, instead of 5000 and no
  cap, and loads split bulks at 16 MB. Large clusters dump and load with more, smaller requests;
  pass `-- --limit=5000 --maxResponseMb=0 --maxBulkBytes=100mb` for the 1.0 behaviour.
- **Pass `--verifyCounts`** if you want `searchdump-multi` to keep re-reading every data file and
  to fail the index when that count disagrees with the child. Without it, a successful child's
  reported count is trusted for the manifest and the summary.
- **Memory estimates are slightly higher** for compressed file output (three extra batches) and
  for `--concurrency 1` encode/send overlap (two extra batches). Peak behaviour is otherwise the
  same.

## Performance

Point-in-time run finished 2026-11-14: every 1.1 commit was built and measured on Elasticsearch 7.10.2
(`es710-plain`), Elasticsearch 8.19.23 (`es819-plain`) and OpenSearch 3.9.0 (`os3-plain`), L0 dataset, three
reps, wall-clock median per case. Full tables (every case, every step, change against the previous step,
noise-aware): [`scripts/perf/results/pit-1.1.md`](../../scripts/perf/results/pit-1.1.md).

**How to read this**

- **Node comparison skipped.** Node elasticdump was not run at the 1.1 steps (`--skip-node`); the "Node" column
  in the report is the beta1-era reference on es710 only.
- **Baselines.** The 1.0.0-beta1 suite baselines come from a separate VM and are not in this report. The only
  beta1 number here is the es710 re-measure of `94e4afb` (step 0), taken *before* the box was restored on
  2026-11-12; it is the one reference for "1.1 vs beta1" and is confounded by that restore (1.1-2 shows
  +26..+48% on es710 restores that es819/os3 cannot confirm). es819 and os3 have no beta1 step, so their tables
  start at 1.1-2 and compare "last vs 1.1-2".
- **One flaky rep.** es710 step 1.1-8: `dump-split-gzip` rep 2 failed its `edverify` check (exit 2) once; the
  step is marked FAIL in the driver, the other reps were fine and the case has no 1.1-8 median in the report.
  It passed at 1.1-9, 1.1-6 and 1.1-7 and on the other engines. It was not reproduced, so it is treated as
  flaky and not attributed to 1.1-8. os3 1.1-3 was re-run after several instant harness failures (isolated
  podman store), then passed.
- A change is bold in the report only when it is at least 5% and twice the larger CV. Many cells move 10-20%
  without that, so read the per-step counts and the repeated pattern across engines, not single cells.

**Step over step** (es710 / es819 / os3; negative = faster)

| Item | Result |
|---|---|
| 1.1-2 targeted refresh | Only measurable on es710 vs beta1: slower (geomean 1.31, 30 of 35 cases slower), restore-none 4.99 s to 7.17 s; restores +39..+48%. Likely box restore, not the code; not confirmed on es819/os3 (first step there). |
| 1.1-1 refresh every 5 s | The clear win, restores and copy: es710 restore-none 7.17 to 5.58 s (-22%), restore-gzip -24%, restore-small -33%; es819 restore-none 6.83 to 4.65 s (-32%), restore-gzip -28%, restore-small -32%; os3 restore-none 6.63 to 4.68 s (-29%), restore-gzip -39%, restore-zstd -41%. Dumps mostly flat (es819 dump-none -13%, os3 -22%). Faster/slower cases: 14/0, 11/2, 12/0. |
| 1.1-3 `--scrollDocSort` (opt-in, off by default) | No intended default effect; es710 restores +3..+15% (restore-zstd +15% bold), es819 restore-gzip -12% and copy -16%, os3 restore-big -23%. Faster/slower 2/6, 7/3, 7/3: noise. |
| 1.1-4 slice cap | es710: dump-scroll-s2 2.35 to 0.88 s (-63%) and s4 1.41 to 0.93 s (-34%), s8 -5%: the intended fix. es819/os3 were already fast uncapped and got slower: dump-scroll-s4 0.46 to 0.67 s (+45%) / 0.43 to 0.69 s (+62%), dump-scroll-s8 0.42 to 0.67 s (+59%) / 0.32 to 0.64 s (+101%). **1.1-4a (2026-11-14) limits the default cap to ES 7.10**, so es819/os3 are back to the pre-1.1-4 uncapped behaviour (code path identical to `--sliceCap=false` there; not re-measured with the full batch). dump-pit-s* is uncapped (es819 s4 -20%, s8 -13%). |
| 1.1-9 compression thread | No clear gain: es710 dump-gzip -5%, dump-zstd +10%; es819 dump-gzip +5%, dump-split-gzip +14% (bold); os3 dump-zstd +24% (bold), dump-split-gzip -6%. Faster/slower 1/3, 7/10, 6/5. |
| 1.1-6 encode overlap | Restores gain on ES: es710 restore-none 5.47 to 4.52 s (-17%), restore-gzip -21%; es819 restore-none -16%, restore-zstd -19%. os3 shows no gain (restore-none +15%, gzip -4%, zstd -6%). Faster/slower 10/1, 8/4, 4/5. |
| 1.1-8 tokio::process | Within noise for `searchdump-multi`: es710 multi-restore-p1 +19% (bold), multi-restore-p8 +13%; es819 multi-restore-p8 +19% (n.s.); os3 multi-restore-p1 +25% (bold), multi-dump-p1 -17%. Faster/slower 3/8, 4/6, 12/4. |
| 1.1-7 child counts | No cost seen without `--verifyCounts`: es710 multi-restore-p1 -11% (bold), es819 multi-restore-p8 -20% and multi-dump-p8 -18% (bold); os3 multi-dump-p4 +49%, p8 +47% (os3 1 faster / 14 slower, most small). |

Net, last step against the first measured one (geomean wall ratio): es710 1.08 vs beta1 (1.31 at 1.1-2),
es819 0.85 and os3 0.91 vs 1.1-2. The README Performance tables in
[README Performance](../../README.md#performance) are still the older 0.1 s-timer numbers and are not 1.1 numbers.

**Second host (`kvmsvr1`), same harness `c4b1b34`, full beta1 on all three engines.** Finished
2026-11-14 16:04 ET. 27/27 steps passed (exit 0). Node 6.125.1 was compared on the beta1 step only.
Tables: [`scripts/perf/results/v1.1-pit-kvmsvr1.md`](../../scripts/perf/results/v1.1-pit-kvmsvr1.md).
This run does not include 1.1-4a.

Versus Node (25 cases each; no compared case slower than Node): ES 7.10 median 2.41×, ES 8.19 1.97×,
OpenSearch 3 1.99×. Versus beta1 at the 1.1 head, geometric-mean wall was 0.866 / 0.956 / 1.011
(below 1 is faster). `restore-none` ended −29% / −33% / −27% versus beta1. The slice cap again
sped ES 7.10 `dump-scroll-s2` (−71% at that step) and slowed ES 8.19 / OpenSearch 3 where extra
slices had been faster (`s8` 0.62 s → 2.05 s and 0.63 s → 1.83 s), which is why 1.1-4a limits the
default cap to ES 7.10. Compression-thread and multi changes still had no clear L0 win.

## See also

- [`CHANGELOG.md`](../../CHANGELOG.md) — 1.1.0
- [`README.md`](../../README.md) — flags, intentional differences, `searchdump-multi`
- [`NODE-DIFFERENCES.md`](../../NODE-DIFFERENCES.md) — #25 refresh targets, #26 cadence, #27 `_doc` sort
- [`project/plan/ROADMAP-1.1-PLAN.md`](../plan/ROADMAP-1.1-PLAN.md) — implementation order and the
  point-in-time test plan
