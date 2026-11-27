# Handoff: searchdump-multi source split (release/1.4)

Self-contained pickup document for finishing the split (Robert t264u: "Perform the code split for multi code";
t265u: save progress so another agent can finish). Times are America/New_York.

## 1. What and where

- Plan: `project/plan/split-1.4/SPLIT-searchdump-multi.md` (copy of `/workspace/split-plan-doc/SPLIT-searchdump-multi.md`;
  the plan's line numbers refer to the 1.3 file; the 1.4 inventory is `project/plan/split-1.4/inventory-1.4.txt`).
- Goal: ONE binary `searchdump-multi`, source moved from `src/bin/searchdump-multi.rs` into
  `src/bin/searchdump-multi/` (`main.rs` + modules). **No behaviour change; help text byte-identical.**
- Branch `release/1.4`, worktree `/workspace/rel-1.4`. Pre-split source: `a4dc857:src/bin/searchdump-multi.rs`
  (5186 lines, 29 unit tests).
- Commit ids continue the 1.4 numbering: `L4-008` = split step 0, `L4-009` = step 1, ... (one commit per step or group).

## 2. Status (updated 2026-11-27 04:45): split complete

| step | commit | state |
|---|---|---|
| 0 safety net (tests only) | 1a5f6df L4-008 | done, gate OK |
| 1 `git mv` -> `searchdump-multi/main.rs` | aeac554 L4-009 | done, gate OK (100 % rename) |
| 2 `reqwest::Method` -> `http::Method` | 05d8801 L4-010 | done, gate OK |
| tooling + this handoff | f76faa5 L4-011 | `project/plan/split-1.4/` |
| 3 cli.rs | c4fa29a L4-012 | done, gate OK |
| 4 sizing.rs, flags.rs | 37b9e3f L4-013 | done, gate OK |
| 5 selection.rs, types.rs | 7c701be L4-014 | done, gate OK |
| 6 files.rs, manifest.rs, discover.rs | 6c1ca1b L4-015 | done, gate OK |
| 7 tls.rs, cluster.rs | 55dd3b9 L4-016 | done, gate OK (ConnOptions seam not added: section 5) |
| 8 child/{mod,stats,probe,runner}.rs | 448b90b L4-017 | done, gate OK |
| 9 ctx.rs, planner.rs | 4cda2b8 L4-018 | done, gate OK |
| 10 dump.rs, load.rs, storage.rs, run_local/mod.rs | 619867c L4-019 | done, gate OK (phase A complete) |
| 11 run_local/setup.rs + `Ctx::new` | b0d36f8 L4-021 | done, gate OK (+2 tests) |
| 12 run_local/dump_plan.rs + dry_run::print_dump_plan | 669082d L4-022 | done, gate OK (+1 test) |
| 13 run_local/load_plan.rs + print_load_plan + report.rs | 6d73cd5 L4-023 | done, gate OK (+2 tests) |
| 14 child data-arg builder | 28e9244 L4-024 | done, gate OK (+2 argv-pinning tests; 36 multi tests); phase B complete |
| 15 visibility + explicit imports | L4-026 | done, gate OK (re-run on the final tree 2026-11-27): glob imports gone, every item `pub(crate)` or private, `#![warn(unreachable_pub)]`, `pub mod sizing` -> `mod sizing` |
| 16 module docs | L4-027 | done: module map in the `main.rs` crate docs, the two intra-doc links broken by the move fixed; `cargo doc --bin searchdump-multi` has no warnings from the binary (the lib's 6 are pre-existing) |
| 17 docs/CHANGELOG | L4-028 | done: CHANGELOG 1.4 "Internal" entry, FEATURES-TODO and examples/README paths, module map in HANDOFF-1.4 (there is no CONTRIBUTING file; ROADMAP plans and `results/local/*` are historical and unchanged); `matrix-md.py --check` OK |
| final verification | L4-029 | done 2026-11-27 04:45 on d869392 (`project/plan/split-1.4/gate-full.sh`): release build; clippy -D warnings (all targets, and `--no-default-features`); `cargo test --workspace` 610 passed / 0 failed (multi bin 36, cli_contract 79, chaos_brk 67, router_mock 17, ...); nextest unit 368/368, contract 131/131; help and -h byte-identical to `tests/fixtures/multi/`; geiger, vet, deny, audit, machete, pins, matrix-md, native-code, notices all pass; IT on `it-os-sec-l4tls` (edrs-1.4-podman, port 19252) 93 passed / 1 ignored (as before the split); release-binary multi smoke dump -> load with `--ca` 5/5 docs; container stopped again. CI on d869392: ci 45972304648 success, security 45972307714 success |

Phase A steps 3-10 were run by `steps.sh 4 10 13` (regenerate, G-fast gate, commit only on GATE OK with 29 multi tests).

Phase B deviations: `dump_plan::start`/`load_plan::start` take `&mut JoinSet`/`&mut early` and return `Option<i32>`
(early exit code) instead of the plan's `Flow` enum (same behaviour, less churn); `spawn_dumps`/`spawn_loads` were
not split out (the spawn loops stay in `start`; a separate fn would take 8 arguments); `Interrupt` and
`cluster_tls` are in `setup.rs`. Every phase B step is reproducible: `phase_b1N.py` on the previous commit + `cargo fmt`.

## 3. How steps 3-10 are produced (mechanical, reproducible)

`project/plan/split-1.4/split.py N DIR` rebuilds the layout for "steps <= N" from the pre-split file
(`git show a4dc857:...`): it chunks the file into top-level items (doc comments/attributes included), moves the
items listed in `MODS` to their module, moves the tests listed in `TESTS` into a `#[cfg(test)] mod tests { use super::*; }`
in that module, and writes `main.rs` with `mod x; use x::*;` (temporary globs, removed in step 15). Each module
starts with `use crate::*;`. Only these edits happen besides the move:

- private top-level items that move become `pub(crate)` (and the fields of the private `struct Ctx`);
- `FIX` list: `Compression::flags`, `Ctx::plan`, `Ctx::stats_of` -> `pub(crate)`; `Cluster.child_base` -> `pub(crate)`
  field (instead of the plan's `child_base()` accessor: less change); `Compression::of_file` (test-only) gets
  `#[allow(dead_code)]` because a crate-root `pub` item was exempt from `dead_code` and a module item is not;
- `pub mod sizing { ... }` becomes `sizing.rs` (body de-indented, item docs become `//!`);
- in moved tests `super::X` -> `crate::X` (the sizing tests have no `use super::*`).
- `split.py 1` reproduces the pre-split file byte for byte; `split.py 2` differs only in the `Method` import.

Per step N (3..10):

```bash
cd /workspace/rel-1.4
project/plan/split-1.4/apply.sh N             # regenerate src/bin/searchdump-multi/ + cargo fmt
project/plan/split-1.4/run-gate.sh gate-sN    # detached G-fast gate, ~5 min; wait for ^rc= in /tmp/gate-sN.log
grep -E '^==|GATE|rc=|ok=|identical' /tmp/gate-sN.log
git add src/bin/searchdump-multi/main.rs src/bin/searchdump-multi/<new files>   # named files only
git commit -m "L4-0NN: split step N (L4-S0N): ..."
```

Module map after step 10 (lines incl. tests, before rustfmt; from `split.py 10`):

| file | lines | items (tests) |
|---|---|---|
| main.rs | 107 | crate docs, `#![forbid(unsafe_code)]`, imports, mod decls, `main` |
| cli.rs | 333 | Direction, Layout, Compression(+impl), Lifecycle, Order, MatchType, Args(+impl) |
| sizing.rs | 443 | former `pub mod sizing` (3 tests) |
| flags.rs | 171 | DATA_FLAGS, flag_name, merge_data_flags(_owned), LIFECYCLE_DROP, strings, cluster_flags (3) |
| selection.rs | 279 | Selector, TargetKind, Target, select_targets, sanitize_dir, check_index_name, rel_dir (3) |
| types.rs | 230 | TYPE_ORDER, CLUSTER_TYPES, Types, node_type_flags, resolve_types (2) |
| files.rs | 383 | CLUSTER_DIR, item_file*, META_SUFFIXES, parse_data_file, rs_named, meta_file, read_text, data_parts, is_part, count_records, mappings_body, remove_data_files, file_name, nonempty, meta_dir (3) |
| manifest.rs | 87 | MANIFEST, IndexEntry, ClusterEntry, Manifest |
| discover.rs | 291 | Found, discover (2) |
| tls.rs | 302 | Secret, parse_secret, Side, ClusterTls(+impl, L4-001), NODE_TLS_ENV (L4-006) (tls_args + 2 L4-001 tests) |
| cluster.rs | 467 | split_url, PARENT_429_RETRIES, parent_call_retryable, parent_retry_delay, Cluster(+impl) (2) |
| child/mod.rs | 9 | `pub(crate) use` of the three below |
| child/stats.rs | 155 | CountSource, ChildStats, parse_stats_file, parse_total_writes, read_tail, child_stats (1) |
| child/probe.rs | 82 | takes_stats_file, accepts_server_limits, accepts_stats_file, find_bin (1) |
| child/runner.rs | 528 | CHILD_GRACE, Runner(+impl), ChildRun, ChildRef, stats_target_index (L4-002), stop_child, send_term, tail (script_runner + 5, incl. l4_002) |
| ctx.rs | 103 | Outcome, Ctx, Pacer (1) |
| planner.rs | 163 | plan_sizes, note_size, limits_max_page, sizing_json |
| dump.rs | 244 | debug_counts, dump_one (incl. `--pageBytes` passthrough, L4-003), dump_cluster |
| load.rs | 238 | load_cluster, load_one, with_part (1) |
| storage.rs | 169 | run, staging_dir (`cfg(feature="opendal")`), chrono_stamp |
| run_local/mod.rs | 559 | run_local (TLS/NODE_TLS warnings, child env) |

Changes against the 1.3-based plan: `tls.rs` holds the existing L4-001 `ClusterTls`/`Secret`/`Side`/`NODE_TLS_ENV`
instead of a new `ConnOptions`; `child/runner.rs` also holds `ChildRun`, `ChildRef`, `stats_target_index` (L4-002);
`--pageBytes` passthrough stays inside `dump_one`.

## 4. Verification

- G-fast per commit: `project/plan/split-1.4/gate-fast.sh` (fmt --check; clippy `--workspace --lib --bins --tests -D warnings`;
  `cargo check --bin searchdump-multi` with `--no-default-features` and `--features opendal`; build; nextest
  `--profile unit -E "$(scripts/sup/flake-policy.py --filter)"` (361 tests); `cargo test --bin searchdump-multi` (29);
  nextest contract `binary(cli_contract)` (79, incl. the 5 `l4_s00_*` safety-net tests); round trip; help diff).
- Help baseline (pre-split, COLUMNS=100): `tests/fixtures/multi/help.txt` (`--help`, 298 lines) and `help-short.txt`
  (`-h`, 149 lines); the contract test `l4_s00_multi_help_text_is_unchanged` and the gate both diff against them.
- G-full (steps 7, 10, 14, 17 and before the push): `cargo build --release`; full nextest unit + contract, chaos_brk,
  router_mock_selftest; IT `cargo test -q --features it --test it` against `it-os-sec-l4tls`
  (`source /workspace/perf-1.4/env.sh; podman start it-os-sec-l4tls`; env in `/tmp/l4-it/env`, port 19252, store
  edrs-1.4-podman); `scripts/sup/geiger-gate.sh`, `cargo vet`, `cargo deny check`, `cargo audit`, `cargo machete`,
  `scripts/it/check-pins.sh`, `scripts/it/matrix-md.py --check`, `scripts/sup/native-code-check.py`,
  `scripts/third-party-notices.py --check`.
- Path references to `src/bin/searchdump-multi.rs`: only docs (examples/README.md, FEATURES-TODO.md, ROADMAP plans,
  HANDOFF-1.4.md; historical `results/local/*` stay). Dockerfile, scripts and workflows use the binary name only.

## 5. Decisions / skipped so far (for Robert)

- Step-0 goldens live in `tests/fixtures/multi/`, not `tests/golden/multi/`: `tests/support/golden.rs` treats every
  directory under `tests/golden/` as an engine recording.
- Step 0(f) round-trip tree golden not added: `scripts/it/multielasticdump-roundtrip.sh` is the gate instead.
  **Pre-existing failure:** on the unmodified a4dc857 binary it gives 47 ok + 1 FAIL ("fake node saw requests it
  does not handle": `scripts/it/fake_es.py` does not serve the 1.2 limits probes / `_stats`). Not in CI. The gate
  requires exactly that baseline; fake_es.py left unchanged.
- Step 7 `ConnOptions`/`child_conn_flags` seam: superseded by L4-001 `ClusterTls` (already the TLS seam). Planned as a
  pure move; `Cluster::new(raw, prefix, &Args, &ClusterTls)` keeps reading `args.http_auth_file`. Changing the
  signature is optional and left out unless it is a pure refactor.
- Phase B steps that would change behaviour or help text are skipped and listed here.

## 6. Gotchas

- Long commands: the tool's background jobs sometimes report "completed" while the process tree keeps running, and
  foreground runs over ~2-3 min may fail with "Command failed to spawn: Aborted". Use `run-gate.sh` (nohup + setsid)
  and poll the log for `^rc=`. Never start a second gate while one runs (shared `target/` and logs).
- Toolchain: `source /workspace/perf-1.4/env.sh` (rustup 1.91.1; `/usr/bin/cargo` 1.85 cannot build).
- Multi has no logger: warnings use `eprintln!`. RouterMock answers the limits probes 404 unless `.serve_probes()`.
- Load split parts must start at `split-0`.
- Disk ~86 % (17 G free; `target/` ~20 G): stop and report above 90 %; no `cargo clean` without approval.

## 7. Standing rules

Never touch `main` or merge into it; no force push, no branch/tag deletion, no PRs, nothing upstream. `git add`
named files only. Never print tokens/secrets (DHI_TOKEN included). Run `scripts/sup/secret-scan.sh` before every
push and check its exit status. Isolated podman stores only (edrs-1.4-podman); don't touch other agents' containers,
the uid 100999 OpenSearch or port 19222. No AWS/B2; `--compare-node` stays optional. Don't delete files outside the
moved source without an explicit yes; leave `oom`, `test_query_elasticdump_rs_test_857109_16.json` and the
eti-opensearch remnants. Uncovered behaviour decisions: conservative default, record it here, don't block.
A 👍 is not a yes.
