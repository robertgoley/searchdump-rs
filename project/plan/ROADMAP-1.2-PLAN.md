# 1.2 plan: server-aware sizing (`--serverLimits`)

Status: **design, not started** (owner request t108u, 2026-11-15 04:09 ET: "Write it up for 1.2 in a
detailed way so if needed, it will remove/rewrite the 1.1-10 change as needed"). 1.1.0 is frozen as it
is: no code change and no re-test for this in 1.1. FEATURES-TODO row: **1.2-1**.

This plan replaces the fixed 1.1-10 `searchdump-multi` data defaults with sizing derived from the
cluster's own limits, and is allowed to delete or rewrite every part of 1.1-10 (section 6).

## 1. Problem and evidence

### 1.1 Small heaps crash or reject fixed-size requests (why 1.1-10 exists)

`results/local/1.1-10.md` (2026-11-14, box): with the 1.0 multi defaults (`--limit=5000
--maxResponseMb=0`, 100 MB bulks)

- bitnamilegacy ES 7.10.2 and 8.6.1 at a 256m heap **die** (`OutOfMemoryError`) on the first dump page of
  20,000 PMC articles (~22 KB JSON, ~12 KB stored) or 3,000 ~80 KB documents; at 384m the 80 KB index still
  kills them;
- official ES 8.19.23 at 256m answers HTTP 429 `circuit_breaking_exception` (`[fetch source]` 154.5 MB over
  the 153.5 MB parent limit) on every retry;
- a load with 5000-document bulks (~105 MB) gets HTTP 429 from `indexing_pressure` (10 % of the heap) even at
  1 GB; 1000-document bulks get 429 from the parent breaker at 256m.

The low-memory set `scripts/load/lowheap.py` (L-818: ES 7.10.2, 8.6.1, 8.19.23, 9.1.2 at 256m/384m, `pmc`
and `fat` datasets) passes 16/16 with the 1.1-10 defaults (release/1.1 and main, `results/local/L-818.md`,
`results/local/L-820.md`, `L-820-lowheap.json`); `--regression` (1.0 flags) fails 23 of 32 runs.

A node that dies with `OutOfMemoryError` sends no 429, so a retry/backoff alone cannot fix this: the
**first** request must already fit.

### 1.2 What the fixed 1.1-10 defaults cost

1.1-10 shrinks every multi data page to 1000 documents (fewer when documents average > 16 KB stored),
keeps the 256 MB response cap and splits load bulks at 16 MB, for every cluster, big or small.

The grokbox 1.1.0 RC run (L0, 3 reps, beta1 case list; engines finished at the time of writing:
es710-plain, es710-secure, es717-plain; the full report will be `scripts/perf/results/v1.1.0-rc-L0.md`
and `results/local/1.1-rc.md`) shows the `many` dataset (16 small-document `logs` indices) multi steps
against beta1 (searchdump wall medians, s):

| Engine | Case | beta1 | 1.1.0 RC | RC/beta1 |
|---|---|---:|---:|---:|
| es710-plain | multi-dump-p4 | 0.503 | 0.702 | 1.40 |
| es710-plain | multi-dump-p8 | 0.304 | 0.445 | 1.46 |
| es710-plain | multi-restore-p4 | 2.212 | 2.847 | 1.29 |
| es710-plain | multi-restore-p8 | 1.916 | 2.610 | 1.36 |
| es710-secure | multi-dump-p8 | 0.404 | **1.545** | 3.82 |
| es710-secure | multi-restore-p8 | 1.817 | 2.226 | 1.23 |
| es717-plain | multi-dump-p4 / p8 | 0.403 / 0.404 | 0.408 / 0.285 | 1.01 / 0.71 |
| es717-plain | multi-restore-p4 / p8 | 2.614 / 1.614 | 2.079 / 1.433 | 0.80 / 0.89 |

p1 steps are 5-14 % faster than beta1 everywhere so far, and es717 shows no multi slowdown, so the cost
is concentrated on ES 7.10 at p4/p8. Every case still beats Node (×1.1-×4.6 on these steps; es710-secure p8 dump only ×1.1).

**es710-secure `multi-dump-p8` 0.40 -> 1.55 s is unexplained.** The per-rep walls are warm-up 0.372 s, then
2.446, 1.545 and 1.046 s (median 1.545); p4 on the same engine is 0.53 s, level with beta1. A deterministic
slowdown (smaller pages) would not show a warm-up at beta1 speed and a falling series. Candidates:

1. the 1.1-10 per-index `_stats/docs,store` probe (`Cluster::avg_doc_bytes`, called from `dump_one` in the
   parent for every index before the child starts) being slow on the TLS cluster, e.g. a new connection and
   handshake per call, or queuing behind 8 children on ES 7.10's small `management` thread pool;
2. 5x more scroll pages per index (6,250 docs per index at 1000/page instead of 5000) with 8 concurrent
   scrolls on ES 7.10 + TLS (scroll-context churn, GC between pages);
3. box noise (this box had a ~3-minute VM stall on 2026-11-14; reps 0-2 run back to back).

The probe does not retry (`ok_json` -> `call` has no retry, failures are ignored by `if let Ok(Some(..))`),
so "refused and backing off" is unlikely; a slow but successful call is the remaining suspect.

**Task 1.2-1a (diagnose first, before writing any 1.2 code):**

- On ES 7.10.2 secure (`scripts/it/up.sh es710-secure`, isolated podman store, nothing else running),
  seed the perf `many` dataset (`suite.py` seeding or `edgen` profile `logs`, 16 indices) and run
  `searchdump-multi --parallel 8` dumps 10x each with: the 1.1.0 defaults; `-- --limit=1000` (pages only,
  probe still runs); `-- --limit=5000` (user `--limit` skips the probe); `--noDefaultDataFlags --
  --limit=5000 --maxResponseMb=0` (1.0 behaviour). Log per-call timings (`RUST_LOG=debug` or a temporary
  timing line) for `_stats/docs,store`, the first scroll page and each child's wall.
- Repeat on es710-plain and es819-secure to separate TLS from ES 7.10.
- Record in `results/local/1.2-1a.md`. Outcome decides whether section 4's "probe once in the parent" is
  a fix or just a cleanup.

## 2. New option `--serverLimits`

Its own flag, not a value on `--sliceCap` (that flag only decides the scroll slice count); same value style
as `--sliceCap=auto|min|total|false`.

```
--serverLimits=off|auto|min|check        (default: auto)
```

| Value | Behaviour |
|---|---|
| `off` | No probing. Fixed defaults exactly as in 1.1.0 (`--limit=1000`, 256 MB response cap, `--maxBulkBytes=16mb`, the 1.1-10 per-index page for large documents in multi). This is the comparison baseline and the escape hatch. |
| `auto` | Probe at startup (section 3), compute a budget, and only ever **tighten** relative to the defaults. A big cluster behaves like `off` (or like the 1.0 page sizes, see 4.2); a small one gets smaller pages / fewer workers / smaller bulks and one log line saying so. User-given values are respected (see below). |
| `min` | The owner's "autosvrmin" idea: size from the **smallest** node with a larger safety margin (e.g. half of `auto`'s budget), for fragile, shared or production clusters. |
| `check` | Probe, print the detected limits and the sizes `auto` (and `min`) would use for this command line, then **exit 0 without dumping or loading**. If the user gave explicit values (`--limit`, `--maxBulkBytes`, `--maxResponseMb`, `--parallel`, `--concurrency`, `--slices`) that exceed the detected limits, print which and **exit 2** (usage error). |

Explicit user values: under `auto`/`min` they win, but a value above the detected safe limit prints a
warning (`--limit=5000 needs ~X MB per page; node N's parent breaker is Y MB`). Only `check` refuses.

Both `searchdump` and `searchdump-multi` get the flag. `searchdump-multi` passes `--serverLimits=off` plus
the computed per-index flags to its children (the parent probes once; section 4.3), so children never probe
again.

## 3. Probes

All read-only, one round trip each, issued once per run (multi: by the parent). Start-up cost target:
< 100 ms on a healthy cluster.

| Probe | Fields used | Notes |
|---|---|---|
| `GET _nodes/stats/jvm,breaker,indexing_pressure?filter_path=nodes.*.name,nodes.*.roles,nodes.*.jvm.mem.heap_max_in_bytes,nodes.*.jvm.mem.heap_used_percent,nodes.*.breakers.parent,nodes.*.breakers.request,nodes.*.breakers.fielddata,nodes.*.indexing_pressure.memory.limit_in_bytes` | heap max / used per node, parent and request breaker `limit_size_in_bytes` and `estimated_size_in_bytes`, indexing-pressure limit | `indexing_pressure` exists on ES >= 7.9. On OpenSearch use `shard_indexing_pressure` (`GET _nodes/stats/shard_indexing_pressure`, OS 1.x+; node-level limit from `shard_indexing_pressure.primary_parameter.node.soft_limit` / the 10 % default when the feature is off). Unknown metric names are rejected with 400 on old versions: send the metric list the detected version supports (version from the cached `GET /`). |
| `GET _nodes/settings?filter_path=nodes.*.name,nodes.*.settings.http.max_content_length` | `http.max_content_length` | default 100mb when not set explicitly. (L2-004 correction: the nodes info API rejects `include_defaults` with a 400 on ES 7.10-9.1; found by the fixture capture.) |
| `GET _nodes/stats/thread_pool/search,write?filter_path=...` or `GET _cat/thread_pool/search,write?h=node_name,name,size,queue_size&format=json` | search / write pool size per node | caps useful concurrency (a 2-core node runs ~4 searches at once). |
| Multi only: `GET _cat/indices?format=json&bytes=b&h=index,docs.count,pri.store.size,pri` (or `GET _stats/docs,store?level=indices`) | per-index primary docs and stored bytes | **replaces the 1.1-10 per-index `_stats` call**: one request for all indices. |

Only data nodes (role `d` or `data_*`) count for search/fetch and indexing budgets; coordinating-only nodes
count for the request breaker of the node the client talks to (unknown behind a load balancer, so use the
smallest node).

### 3.1 Budget

- **Dump (per in-flight page):** `page_budget = min(parent_free, request_limit) * k` where
  `parent_free = parent.limit - parent.estimated` on the smallest data node and `k` = 0.25 (`auto`) or
  0.125 (`min`); and never above the response cap (`--maxResponseMb`, default 256 MB). The in-flight total
  is `page_budget x slices x concurrency x parallel workers` and must fit `min(parent_free) * 0.5` (`auto`)
  or `0.25` (`min`); the sizing policy (section 4) picks workers/pages to satisfy it.
- **Page documents:** `limit = clamp(page_budget / (avg_source_bytes x expansion), 1, max_page)` where
  `avg_source_bytes` comes from stored bytes / docs and `expansion` (fetched `_source` + JSON + highlighting
  overhead vs stored) starts at 3 and is tuned from the 1.2-1b matrix. `max_page` = 5000 (the 1.0 value) so
  big clusters get beta1 page sizes back.
- **Load (per bulk):** `bulk_bytes = min(indexing_pressure_limit, parent_free) * k / (concurrency x parallel)`,
  and always `<= http.max_content_length - 1 MB` and `<= --maxBulkBytes` when the user gave one.
- Heap use moves during a long run, so these are starting points; mid-run backoff (section 5) handles the
  rest.

### 3.2 Fallbacks (never fail the run because of probing)

| Situation | Detection | Behaviour |
|---|---|---|
| No `monitor` cluster privilege | 403 on `_nodes/*` | 1.1.0 fixed defaults + one warning: "server limits unknown (no monitor privilege); using fixed defaults". |
| Stats hidden / partial | missing fields, 400 on a metric | use what is present; missing pieces fall back to the fixed default for that piece. |
| Elastic Serverless, OpenSearch Serverless | no `_nodes` API (404/410/400), `GET /` build flavor `serverless` or AOSS endpoint | fixed defaults + warning. |
| AWS OpenSearch Service / legacy Amazon ES | host `*.es.amazonaws.com`, `*.aos.*.on.aws`, or the `x-amzn-*` / `AwsOpenSearch` response headers; `http.max_content_length` not exposed | apply AWS's published per-instance-type HTTP payload cap (10 MiB on the small instance types, 100 MiB on larger ones) when the instance type is not known, default to the 10 MiB cap and say so; the user can raise it with `--maxBulkBytes`. Keep the table in one Rust const with a link to the AWS doc and its retrieval date. |
| Behind a proxy that hides node stats | as 403/404 | fixed defaults + warning. |
| Probe slower than a deadline (e.g. 5 s) | timer | abandon, fixed defaults + warning. |

## 4. Sizing policy

### 4.1 Prefer fewer workers to smaller pages (multi)

Server memory load is roughly `workers x page bytes`. Two workers at 5000 docs cost about what 8 workers at
1250 cost, with a quarter of the round trips, query parse / fetch-phase setup, TLS records and GC pauses.
Extra workers also do not help a small node: its search pool (~1.5 x cores + 1) queues them. So, in order:

1. Start from the user's `--parallel` (multi default **2**; the perf suite also runs p4/p8) and `max_page`.
2. If the in-flight total exceeds the budget, **lower parallel workers first**, down to 2 (or to the
   search-pool size if smaller), keeping the page.
3. Only if 2 workers still do not fit, **shrink the page**.
4. Independently, an index whose documents are so large that **one** page exceeds `page_budget` gets a
   smaller page (the 1.1-10 large-document rule, now in bytes).
5. **Many small indices:** when an index holds fewer documents than one page, page size is irrelevant and
   per-index setup (mapping, settings, aliases, `_count`) dominates; keep the high worker count for those
   indices (budget them at their actual size, not `max_page`).

The multi parent schedules indices against a byte budget (a weighted semaphore: each running index holds
`its page bytes x its concurrency x slices` permits) instead of a plain `--parallel` count. `--parallel` stays
the upper bound.

### 4.2 Byte-based pages

Pages are sized in target response bytes (section 3.1) instead of a fixed document count, always under the
256 MB response cap. `searchdump` gets an internal option (not user-facing in 1.2, or `--pageBytes` if the
owner wants it) that converts bytes to `size` per index from the parent's stats. Small-document indices
return to 5000 per page on clusters that can take it, which should recover the beta1 multi timings.

### 4.3 Probe once in the parent

The multi parent issues the single `_cat/indices` (or `_stats?level=indices`) call and the node probes,
computes every index's page and bulk, and starts children with explicit `--limit=N --maxBulkBytes=M
--serverLimits=off`. No per-index `_stats` call, no per-child probe, fewer TLS handshakes on secure
clusters (suspect 1 in 1.1).

### 4.4 Load side

Bulk bytes from 3.1, divided across `concurrency x parallel`; never above `http.max_content_length` (or the
AWS cap). The worker-first rule applies to loads as well (fewer parallel loads with bigger bulks, until the
indexing-pressure limit is the constraint).

## 5. Mid-run: HTTP 429 handling (the deferred 1.1 item)

Startup probing cannot see heap pressure that builds up later or other tenants' load. In `searchdump`:

- On HTTP 429 `circuit_breaking_exception` for a search/scroll page: **halve the page** (for scroll: next
  `_search/scroll` cannot change `size`, so restart the slice from the last `_id`/sort value with a PIT or
  `search_after` where available, or keep the size and back off on plain scroll), back off with jitter,
  and log once per index. Floor at 1 document; after N consecutive 429s at the floor, exit 4 (rerun-safe).
- On 429 `es_rejected_execution_exception` / indexing pressure for a bulk: split the bulk in half and retry
  the halves (no duplicate writes: only unacknowledged items are resent).
- Never grow back above the startup size within a run (keeps behaviour predictable).
- Applies with `--serverLimits=off` too (it is the 1.1 deferred fix), so it can ship first if 1.2 is split.

## 6. What happens to 1.1-10 (remove / rewrite list)

1.1-10 is commit `f22ea92` (release/1.1), on main since L-820 (`0d7d518`). File: `src/bin/searchdump-multi.rs`
(line numbers as of `8952185`).

| 1.1-10 piece | Where | 1.2 action |
|---|---|---|
| `DATA_FLAGS` `--limit=1000` and `--maxBulkBytes=16mb` (and the doc comment above it, ~l.117-133) | `const DATA_FLAGS` | **Rewrite.** `DATA_FLAGS` keeps only run-control flags (`--scrollTime=10m`, `--retryAttempts=5`, `--retryDelay=2000`, `--timeout=300000`). Page and bulk come from the sizing module. With `--serverLimits=off` the module returns exactly `--limit=1000`, `--maxBulkBytes=16mb` and the 1.1-10 large-document page, so 1.1.0 behaviour is reproducible. |
| `DEFAULT_DATA_LIMIT` (1000), `PAGE_STORE_BUDGET` (16 MiB) | consts ~l.134-140 | **Move** into the sizing module as the `off` profile constants. |
| `fn page_limit_for(avg_doc_bytes)` | ~l.145 | **Replace** by the byte-budget page function (section 3.1/4.2); keep a thin `off`-profile wrapper with identical results. |
| `Cluster::avg_doc_bytes(index)` (`GET {index}/_stats/docs,store`) | ~l.1357 | **Delete.** Replaced by one `_cat/indices` / `_stats?level=indices` call in the parent (4.3). |
| The per-index block in `dump_one` that finds `--limit=1000`, checks the user did not pass `--limit`, calls `avg_doc_bytes` and rewrites the flag, printing `note: {index}: documents average ...` | ~l.1833-1852 | **Delete.** The parent passes each child its computed `--limit`/`--maxBulkBytes`; the note becomes part of the one "server limits" line plus a per-index line only when an index is shrunk. |
| `merge_data_flags` (a user flag after `--` replaces the default of the same name) | ~l.163 | **Keep.** User values still win. |
| `--noDefaultDataFlags` | ~l.366 | **Keep**; implies `--serverLimits=off` for the data flags. |
| Test `default_data_flags_are_small_heap_safe` | tests mod ~l.2934 | **Replace** by `server_limits_off_reproduces_1_1_0_flags` (same assertions against the `off` profile) and `run_control_flags_unchanged`. |
| Test `page_limit_shrinks_for_large_documents` | ~l.2947 | **Keep the table** as the `off`-profile test; add byte-budget tests (`auto` with a 256 MB-heap probe fixture, `min`, huge-document floor = 1). |
| Test in the merge-flags test asserting `--limit=1000` is replaced | ~l.2917 | **Update** to the new default source. |
| `scripts/load/lowheap.py` default mode (1.1 flags) and `--regression` (1.0 flags) | scripts/load | **Keep**; add `--serverLimits=off` mode (= 1.1.0) and make the default mode exercise `auto`. |
| Docs: CHANGELOG 1.1.0 entry, README "Tuning"/multi defaults, `examples/README.md` "Minimum resources" and the small-heap overrides in `examples/*/multi-index-*`, `scripts/load/README.md`, RELEASE-NOTES-1.1 | docs | **Rewrite** for 1.2 (the examples' explicit `-- --limit=500 --maxResponseMb=64` overrides can go once `auto` is the default and passes lowheap). |

Single-index `searchdump` keeps its own defaults (`--limit=100`, `--concurrency=1`, 256 MB response cap,
100 MB bulks); under `auto` it applies the same budget (tighten only).

## 7. Logging and manifest

- One line at start (stderr, and in `--statsFile` output):
  `server limits: 1 data node, heap 256 MB (61 % used), parent breaker 243 MB (free 95 MB), request 154 MB, indexing pressure 25 MB, http.max_content_length 100 MB, search pool 4 -> page 400 docs (~6 MB), bulk 4 MB, parallel 2 (asked 8) [auto]`
  or `server limits: unknown (403 on _nodes/stats: no monitor privilege) -> fixed 1.1 defaults [auto]`.
- Multi manifest (`multielasticdump.json`): a `server_limits` object: `mode`, `source`
  (`probed|fallback-403|fallback-serverless|aws-table|off`), per-node heap/breaker/indexing-pressure/
  max_content_length/search pool, the computed budget, and per index the chosen `limit`, `max_bulk_bytes`
  and the reason (`default|large-docs|budget`). `searchdump --statsFile` gets the same top-level object.
- Exit codes unchanged; `check` uses 0 and 2 only.

## 8. Test plan and acceptance

1. **Diagnosis** 1.2-1a (section 1.2) done and recorded before code.
2. **Unit:** budget math (fixtures from real `_nodes/stats` bodies of ES 7.10.2, 8.6.1, 8.19.23, 9.1.2,
   OS 1.3, 2.x, 3 at 256m/384m/1g, stored under `tests/fixtures/server-limits/`), worker-first policy,
   byte pages, the `off` profile equals 1.1.0.
3. **fake_es / contract tests:** 403 on `_nodes/*` -> fallback + warning, exit 0; serverless (`_nodes`
   404) -> fallback; AWS host -> 10 MiB cap; probe timeout -> fallback; `check` exit 0 output and exit 2 on
   an explicit `--limit` over the limit; multi parent makes exactly one per-run stats call (no per-index
   `_stats`).
4. **Acceptance: `scripts/load/lowheap.py` with defaults** (`auto`): every 256m and 384m case passes on ES
   7.10.2, 8.6.1, 8.19.23, 9.1.2, **plus OpenSearch 1.3, 2.x and 3** small heaps (new cells; OpenSearch 2/3
   probably need 384m/512m as their floor, record what boots). `--serverLimits=off` must reproduce the 1.1.0
   results; `--regression` still fails.
5. **Worker/page trade-off matrix** (1.2-1b) on the low-heap engines, both datasets, dump and load:
   `--parallel 2 --limit 5000`, `4 x 2500`, `8 x 1000`, plus `auto`; wall, node survival, 429 count. Feeds
   the `expansion` constant and confirms 4.1. About 30 minutes on the box.
6. **Perf:** the L0 16-engine suite (same harness, beta1 case list) at the 1.2 candidate: no regression
   against the 1.1.0 RC report on any non-multi case (within noise), multi p4/p8 back at or below beta1,
   es710-secure `multi-dump-p8` back near 0.4 s. kvmsvr1 run per `project/HANDOFF-1.1.md` 5b.
7. **IT:** `scripts/it/run.sh` on es710-plain, es819-plain, elasticsearch-plain, opensearch-plain and one
   secure profile (probe through TLS + auth); the elasticsearch-secure alias 403 stays a known issue.

## 9. Open questions for Robert

Owner t196u (2026-11-15): "Will have to review and provide answers later for 9". Until then the 1.2-1
phases use the choices below, each **PROVISIONAL pending Robert**; every one is a constant or a default
that changes in one place.

1. Default `auto` in 1.2, or ship `off` as default for one release and make `auto` opt-in?
   **PROVISIONAL: `auto` is the default**, trivially switchable (`limits::DEFAULT_MODE`, one line;
   `--serverLimits=off` restores 1.1.0 exactly).
2. `min` margin: half of `auto` (proposal), or a fixed fraction of the heap?
   **PROVISIONAL: half of `auto`** (`limits::fractions`).
3. Should `check` also be a separate subcommand (`searchdump-multi check-limits`) for discoverability?
   **PROVISIONAL: no subcommand**; `--serverLimits=check` only.
4. User-facing `--pageBytes`, or keep byte paging internal?
   **PROVISIONAL: internal** (no new flag).
5. AWS: hard-code the published payload table, or always use the 10 MiB floor unless the user raises it?
   **PROVISIONAL: the 10 MiB floor** (`limits::AWS_PAYLOAD_FLOOR`) unless the user gives
   `--maxBulkBytes`.
6. Should the 429 page-halving (section 5) ship alone first (1.1.1 or 1.2-early), since it is independent?
   **PROVISIONAL: ships within 1.2** (phase 1.2-1c, commit L2-003); no 1.1.1.
7. OpenSearch small-heap floor for the acceptance set: 256m like ES, or the smallest heap OS 2/3 boot with?
   **PROVISIONAL: the smallest heap OpenSearch boots with that is >= 256m**, measured by
   `scripts/load/limits_fixtures.py` (boot.json; recorded in results/local/L2-004.md and L2-006.md).

## 10. Risks

- **Probe cost / permissions:** extra startup calls; mitigated by one call each, a deadline and silent-
  but-logged fallback. Least-privilege dump users lack `monitor` and will always see the fallback line.
- **Stats are a snapshot:** a busy cluster can be fine at start and tight later; section 5 covers it.
- **Over-tightening on healthy clusters** would be a new perf regression; `auto` is capped at `max_page`
  = 5000 and the L0 suite gate catches it.
- **Behind load balancers** the coordinating node is unknown; using the smallest node is conservative.
- **Version drift:** metric names differ (ES < 7.9 has no `indexing_pressure`; OS has
  `shard_indexing_pressure`); fixtures per version and a 400-tolerant probe.
- **Scope creep:** keep 1.2-1 to sizing; no auto-tuning of `--slices` beyond the existing `--sliceCap`.

## 11. Phases and rough sizing

| Phase | Content | Size |
|---|---|---|
| 1.2-1a | Diagnose the es710-secure p8 slowdown (section 1.2) — **done (L2-001)** | S (0.5 day incl. write-up) |
| 1.2-1b | Worker/page trade-off matrix on low-heap engines | S |
| 1.2-1c | 429 page-halving / bulk-splitting in `searchdump` (section 5), with `off` default sizing | M |
| 1.2-1d | Probe module + budget + `--serverLimits` (`off`/`auto`/`min`/`check`) in `searchdump`; fixtures; fallbacks incl. AWS | M-L |
| 1.2-1e | Multi: parent single stats call, byte-budget scheduler, remove the 1.1-10 pieces (section 6), manifest | M |
| 1.2-1f | lowheap.py `auto`/`off` modes + OpenSearch cells; docs and examples rewrite | S-M |
| 1.2-1g | Gates, IT, L0 16-engine perf vs 1.1.0 RC, kvmsvr1 handoff | M (perf run ~10 h on the box) |
| 1.2-1h | Examples and README quick start led by `--serverLimits` as the main or only sizing option (section 12) | S-M |
| 1.2-3 | Broken output pipe (`\| head`): stop cleanly instead of aborting; exit code undecided pending detailed review (t201u); output is truncated, so 0 is not assumed | S |

Each phase is one `L-NNN:` commit with its own results file. 1.1.0 is not changed by any of this.

## 12. Examples: server limits as the main sizing option (phase 1.2-1h)

Owner request t109u (2026-11-15 04:22 ET): "Add notes to add examples for the 1.2 change that utilize the
server side limits as the main or only constraint cli option. Could be the safe simple works in most if not
all type of example." Done after 1.2-1d/e, when `--serverLimits` exists; docs and scripts only.

### 12.1 Goal

A **safe simple** recipe that works on almost any cluster (256 MB test node to big production cluster,
Elasticsearch 7.10-9 and OpenSearch 1-3, plain or TLS + auth) with **no** `--limit`, `--maxBulkBytes`,
`--maxResponseMb`, `--parallel`, `--concurrency` or `--slices` tuning. `--serverLimits=auto` (the 1.2
default, written out in the examples so it is visible) is the only sizing option; `--serverLimits=min` for
fragile or shared clusters. Tuned recipes stay, but become the "when to override" path.

### 12.2 New and changed files

| Location | Change |
|---|---|
| `examples/bash/safe-simple-dump.sh`, `safe-simple-restore.sh` (new) | `searchdump` single index: preflight `--serverLimits=check`, then dump (settings, mapping, data) / restore with only URLs, files, auth via env or `--httpAuthFile`, and `--serverLimits=${SERVER_LIMITS:-auto}`. |
| `examples/bash/safe-simple-multi-dump.sh`, `safe-simple-multi-restore.sh` (new) | `searchdump-multi` same pattern: `--match`, output dir, `--serverLimits`; **no** `-- --limit=... --maxBulkBytes=...` overrides and no `--parallel` (the planner chooses workers first, then pages). |
| `examples/docker-podman/safe-simple-*.sh` (new, same four) | Same commands through `run_image` in `_common.sh`; image `ghcr.io/robertgoley/searchdump-rs:1.2.0-dhi-debian`. |
| `examples/kubernetes/safe-simple-dump.yaml`, `safe-simple-restore.yaml`, `safe-simple-multi-dump.yaml`, `safe-simple-multi-restore.yaml` (new) | Job with an init container (or first `sh` step) running `--serverLimits=check`; the Job fails fast on exit 2. Credentials only from a Secret as blank env references; resources sized for the tool only (it no longer needs to guess the cluster). |
| Existing `minimum-resources-*`, `multi-index-*`, `very-large-index-*`, `cluster-to-cluster-copy*` (bash, docker-podman, kubernetes) | Drop the hand-tuned small-heap overrides (`-- --limit=500 --maxResponseMb=64`, `MAX_BULK_BYTES=16mb`, `--parallel=1`) in favour of `--serverLimits=min` (minimum-resources) or `auto`; keep a commented "manual override" block showing the old flags. `fastest-*` keep their tuning but add `--serverLimits=auto` so tuning above a safe limit warns. |
| `examples/README.md` | New first section "Safe simple (recommended)" with the four commands, the preflight, the fallback table (12.4) and "when to override" (12.5); the case table gets a "sizing" column (`auto` / `min` / manual). |
| `README.md` "Quick usage" | **Lead with the safe-simple recipe** (one `check` line, one dump, one restore, one multi dump) before any tuning flags; move tuning to "Tuning". |
| `RELEASE-NOTES-1.2.md` upgrade guide | "Remove your 1.1 small-heap overrides; `--serverLimits=auto` replaces them; `--serverLimits=off` restores 1.1.0 sizing." |

### 12.3 Preflight step (`--serverLimits=check`)

Every safe-simple script starts with:

```
searchdump --input="$SRC" --output=/dev/null --serverLimits=check   # or searchdump-multi ... --serverLimits=check
rc=$?
case $rc in
  0) ;;                                  # limits read (or fallback); sizes printed; go on
  2) echo "explicit flags exceed what the cluster can take; see above" >&2; exit 2 ;;
  *) echo "preflight failed ($rc)" >&2; exit "$rc" ;;
esac
```

The printed "server limits:" line is kept in the job log. Scripts never parse it (format may change); they
only branch on the exit code. Exit 0 also covers the fallback cases, so preflight never blocks a cluster
that hides its stats.

### 12.4 Fallback notes in the examples

| Case | What the user sees | What the example says |
|---|---|---|
| Missing `monitor` privilege (403 on `_nodes/*`) | warning "server limits unknown (no monitor privilege); using fixed defaults" | Grant the minimal read-only role. Elasticsearch: cluster privilege `monitor`, index privileges `read` and `view_index_metadata` on the dumped indices (restore: `create_index`, `write`, `manage` on the targets as today). OpenSearch security plugin: action group `cluster_monitor` plus the index read groups (`read`, `indices:admin/mappings/get`, `indices:admin/get`). Show a role JSON with blank placeholders only. Without it the run still works with the 1.1 fixed defaults. |
| AWS OpenSearch Service / legacy Amazon ES | line says `source=aws-table`, bulk capped at the instance type's HTTP payload limit (10 MiB on small types) | Nothing to do; raise `--maxBulkBytes` only if the domain's instance type allows 100 MiB. No AWS calls are made by the tool (and no AWS runs in our tests without Robert's approval). |
| Elastic Serverless, OpenSearch Serverless | `source=fallback-serverless`, fixed defaults | Nothing to do; the service throttles with 429, which the mid-run backoff (section 5) handles. |
| Proxy hides node stats / probe timeout | fallback warning | Same as the 403 case. |

### 12.5 When to override manually

Documented in `examples/README.md` and as comments in the scripts:

- Fastest possible copy between big, dedicated clusters, where you know the limits (`fastest-*`).
- Least-privilege accounts that cannot get `monitor` and a known-small cluster: use `--serverLimits=min`
  semantics by hand (`--limit=500 --maxBulkBytes=8mb --parallel=1`), since the fallback assumes a normal
  cluster.
- Reproducing 1.1.0 behaviour for comparison: `--serverLimits=off`.
- An index with single documents larger than the budget: `--limit=1` or `--maxDocMb` as today.
- User values still win under `auto`/`min` (with a warning when over the detected limit); only `check`
  refuses them.

### 12.6 Validation (part of the 1.2 acceptance, section 8)

- `shellcheck` (0.11+) on all `examples/bash/*.sh` and `examples/docker-podman/*.sh`: clean.
- `kubeconform -strict` on all `examples/kubernetes/*.yaml`: all valid.
- **Live run** of each safe-simple bash script (single and multi, dump and restore) against a low-heap engine
  in the lowheap.py style: bitnamilegacy ES 7.10.2 and official 8.19.23 at 256m, plus one OpenSearch small
  heap, both `pmc` and `fat` datasets; pass = exit 0, node alive, document counts and digests equal, and
  the preflight printed a "server limits:" line with `source=probed`. One run with a user lacking `monitor`
  (secure profile) must pass with the fallback warning. Docker/Podman scripts: one live run on the image
  built from the 1.2 candidate. Record in the 1.2-1h results file.
- `scripts/sup/secret-scan.sh` OK; credentials only as blank variable references.
- Optional: add the safe-simple multi dump/restore to `lowheap.py` as a mode so the examples' exact command
  lines are regression-tested.


## 13. Automax: growing limits above the defaults (DESIGN ONLY / NOT IMPLEMENTED)

Owner t196u (2026-11-15): "Think about an automax option that can grow limits but do not implement now."
Nothing in 1.2-1 implements this; it is a design for a later release (FEATURES-TODO 1.2-2).

**What.** `--serverLimits=automax` (opt-in only; never the default, never chosen by `auto`): like `auto`,
but sizes may also *grow* during a run, above `auto`'s starting values, when the cluster shows headroom.

**What may grow, and when.**

| Knob | Starts at | Grows when | Step | Ceiling |
|---|---|---|---|---|
| Page size (`--limit`, search) | `auto`'s page | N=4 consecutive pages with no 429, response bytes < 50 % of the page budget, and a re-probe shows parent-breaker headroom >= 2x the next page | x1.5 | `min(10000 = index.max_result_window, page budget / (avg x EXPANSION), response cap)`; multi: 5000 per worker |
| Workers (multi `--parallel` in flight) | `auto`'s fit (worker-first) | same as above for the whole run, and the search thread pool queue is empty in a re-probe (`_cat/thread_pool/search` `queue=0`, `rejected` unchanged) | +1 | the user's `--parallel` if given, else `min(search pool size, 2 x data nodes x cores reported)`; never above the byte budget |
| Bulk size (`--maxBulkBytes`) | `auto`'s bulk | N=4 bulks with no item 429/503, `indexing_pressure` current < 25 % of its limit, `write` queue empty | x1.5 | `min(http.max_content_length - 1 MB, AWS floor on AWS, indexing-pressure budget / workers, 100 MB)` |

**Safety rules.**

- Never above the probed server limits: every ceiling is computed from the same probe as `auto` and re-checked
  by a cheap re-probe (`_nodes/stats/breaker,indexing_pressure` with `filter_path`, at most once per 30 s,
  same 5 s deadline). A failed or fallback re-probe freezes growth (no growth without fresh numbers).
- Headroom first: growth needs the *next* step to fit with 2x margin in the current parent-breaker free
  bytes (`limit - estimated`), not just the start-of-run numbers.
- 429 feedback wins: any size 429 (section 5) halves the knob (as today) **and** stops growth of that knob for
  the rest of the run; a queue-full 429 freezes worker growth. Growth never undoes a halving.
- Grow slowly (x1.5, then wait N clean steps), shrink fast (/2).
- One knob at a time, workers last: page first, then bulk, then workers, so a regression is attributable.
- Every change is logged once (`server limits: automax: page 5000 -> 7500 (headroom 410 MB)`) and recorded in
  the statsFile / manifest `server_limits.automax` history.

**Interaction with explicit values.** An explicit user value is a hard ceiling **and** the start for that
knob only if it fits the limits; automax never raises a knob the user set (`--limit`, `--maxBulkBytes`,
`--parallel`, `-- --limit=N` in multi). Unset knobs grow. `--noDefaultDataFlags` implies `off` as now.

**Interaction with `check`.** `--serverLimits=check` stays read-only; with a new `--serverLimits=check`
line `automax ceilings: page <= N, bulk <= M, workers <= P` it prints how far automax could grow, so a user
can preview it. `automax` itself never exits 2; explicit too-large values warn as in `auto`.

**Tests it would need.**

- Unit: growth step and ceilings per fixture (es710/es819/es912/os2/os3 at 256m/1024m); never above
  `max_content_length`, the AWS floor, `max_result_window`, the user's value; growth frozen after a 429.
- Mock (RouterMock): pages grow after N clean pages; a breaker 429 halves and freezes; a re-probe 403 freezes;
  bulk grows and stops at `max_content_length - 1 MB`.
- Chaos: 429 storm after growth; slow re-probe (deadline); flapping headroom (no oscillation: frozen after
  the first back-off).
- Load (lowheap.py `--mode automax`): every 256m/384m cell still passes with `node_alive` and equality; no new
  circuit_breaking counts vs `auto`.
- Perf (L0/L1): automax >= auto on 1024m+ engines and never slower than `off` by more than noise.
