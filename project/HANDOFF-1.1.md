# Cold-start handoff: release/1.1 and the beta1 perf suite

**Item checks done (2026-11-14 19:20 America/New_York).** All five checks the suite does not cover passed on 85d4185 (ES 8.19.23): `--scrollDocSort` A/B with Node, golden `--check` (25 cases, zero diff), the `--sliceCap=false` slicing tests on 4 profiles, Ctrl-C of `searchdump-multi --parallel 4` (exit 130, 4 children named), and `--verifyCounts` on 300 indices. `results/local/1.1-item-checks.md`.

**Today's 1.0-line fixes merged into 1.1 (2026-11-14, owner decision t91u).** Cherry-picked from `main`
(`-x` trailers) on top of `b849877`: L-645b `8798ac5`, L-804a `8bcc92c`, L-808n `b158898`, L-810i `15f049f`,
L-808o `b1af6c0`, L-808p `7c5391f`, L-808q `f875663`, L-808r `1f528d7`, L-804b `7e6b7e6`, L-809 `7b89522`,
L-810 `311097c`, L-812 `3552a8b`, L-804c `20e3832`. The main-only tracking docs (TESTING-*, PROGRESS, RESUME,
handoff SHA256SUMS) were left as they are on this branch, and main's CHANGELOG entries were rewritten into the
1.1 section. Code conflicts, all in `src/pipeline/bulk.rs` against the 1.1-6 encode/send pipeline: L-808n (1.1-6
already split lines with memchr; `nonempty_lines` and the depth shortcut added), L-808o (`check_and_parse` /
`encode_top` wired into `encode_batch`, 1.1-4 `batch_indices` kept), L-808p (move-not-copy in `send_batch`),
L-808r (`write_batch(Bytes)` feeds `encode_batch` / `process_batch` without `copy_from_slice`), L-804b
(`rerun_safety` on the `send_batch` error path that keeps the written-index bookkeeping; the 1.1-7 stats-file
test gets `..Default::default()`), L-812 (the routing hint uses 1.1-6's shared `Arc<AtomicBool>`, so it is
printed once across all `--concurrency` workers; on `main` each worker has its own flag). README: the L-810i SFTP
text replaces the 1.0.0-beta1 known-issue paragraphs. Not re-measured with the PIT batch.
Tests on the merged tree: fmt, strict clippy, unit/contract/chaos 537 passed 0 failed; `scripts/it/run.sh` with
the isolated podman store: es819-plain (8.19.23), elasticsearch-plain (9.1.2), opensearch-plain (2.19.7) and
es710-plain (7.10.2) 90 passed, 0 failed, 1 ignored each (es710 run alone with a fresh cross-engine directory).
Still open for 1.1: the five item checks below, 1.1-10 (FEATURES-TODO `13c0725`), the examples update, the
16-engine RC run (planned on kvmsvr1), and the owner's call on the `security` image job ("undocumented
executables" pulled in by openssh-client since L-810i).

**kvmsvr1 confirmation (2026-11-14 16:04 America/New_York).** A second full PIT
batch on harness `c4b1b34` (before 1.1-4a) passed 27/27, including beta1 on
es710, es819 and os3, with Node 6.125.1 on the beta1 step. Report:
`scripts/perf/results/v1.1-pit-kvmsvr1.md`. It confirms the grokbox shape:
refresh cadence is the restore win; the slice cap helps ES 7.10 and slows
ES 8.19 / OS 3 over-sliced scrolls. It does not re-measure 1.1-4a (`2fe3d0b`).

**PIT batch finished (2026-11-14 02:47 America/New_York).** All 3 engines x 9
steps ran (`es710-plain`, `es819-plain`, `os3-plain`; Node skipped).
`pit.sh aggregate` was run and the report is committed as
`scripts/perf/results/pit-1.1.md`; the 1.1 release notes' Performance section
and the CHANGELOG are filled in. Final per-step PASS everywhere except es710
1.1-8 (one flaky `dump-split-gzip` rep 2 edverify exit 2, not reproduced).
os3 1.1-3 needed re-runs after harness fast-failures. es819/os3 have no
beta1 step; the es710 beta1 number predates the 2026-11-12 box restore.
Next: the item checks listed in `ROADMAP-1.1-PLAN.md` and the 16-engine RC run
(`scripts/perf/results/v1.1.0-rc-L0.md`). The PIT watch routine has nothing left
to run or relaunch. Nothing is tagged, no PR is open; `release/1.1` is a normal
push (no force).

**Branch status (2026-11-12 01:30 America/New_York):** phases 1 and 2 of
the point-in-time batch are **done and green** on the local `release/1.1`
(head = the commit that adds this note). That history differs from
`origin/release/1.1` (`a93829a`) because two compile/lint fixes were folded
in with autosquash, so it is pushed to **`origin/wip/1.1-fixups`** instead
of force-pushing `release/1.1`. Robert decides whether `release/1.1` is
moved to it (force push) before phase 3. Folded fixups:

- `retrieval(1.1-3)` (`87e7b38` -> `87d1781`): `src/retrieval/search_body.rs`
  missed `use sonic_rs::JsonContainerTrait;` (E0599 `as_object`). This was
  the only `cargo check` failure.
- `retrieval(1.1-4)` (`3d3c092` -> `aee5dfb`): clippy `type_complexity` in
  `tests/cli_contract.rs` (slice-cap case table); a local `type Case` alias.

Every later hash changed; this file and `ROADMAP-1.1-PLAN.md` carry the new
ones. Results (Rust 1.91.1, all at the new head unless noted):

| Gate | Result |
|---|---|
| `pit.sh check` (cargo check --all-targets --locked at every step, beta1 + 8 items) | 9/9 compile, 0 warnings |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets --locked -D warnings`; CI form `--workspace --lib --bins --tests --features searchdump-rs/load`; `--workspace --all-targets --features searchdump-rs/load` | clean (after the 1.1-4 fixup) |
| `cargo test --locked --workspace` | 529 passed, 0 failed |
| `cargo nextest run --workspace` (default = unit + contract + chaos) | 522 passed |
| `scripts/perf` selftest | 24 OK |
| deny, vet, machete, flake-policy, native-code, check-pins, matrix-md, hygiene, lint-workflows (actionlint+zizmor), secret-scan, third-party notices, check-clean-tree | all OK |
| `scripts/it/run.sh opensearch-plain` / `opensearch-secure` / `es710-plain` | 90+31 / 90 / 90+31 passed, 0 failed |

The es710-plain run fails `cross::file_dump_from_the_other_engine_restores_here`
only when it runs after opensearch-plain in the same `IT_CROSS_DIR`: the
OpenSearch ISM export is restored onto the ES 7.10 OSS build and gets the
OSS "ILM is an X-Pack feature" refusal instead of the "ISM -> ILM" one the
test expects. Neither `tests/it` cross code nor `src/meta` changed in 1.1, so
this is a pre-existing test gap (CI runs each profile in its own job). With
a fresh cross dir, es710-plain passes 90/90.

Box note: the shared podman store (`~/.local/share/containers/storage`) has
missing overlay layers after the box restore (opensearch 2.19.7 and ES 7.10
images fail to mount). The IT runs used an isolated store via the wrapper
`/workspace/rel11-it/bin/podman` (`--root ~/.local/share/rel11-podman`).
`/workspace/perf-b1`, `/workspace/fix-wt` and `/workspace/node-ed` are also
gone after the restore, so phase 4's `PIT_ENV=/workspace/perf-b1/env.sh` and
the beta1 isolated store must be recreated before `pit.sh run`.

Phases 3-5 (`pit.sh build`, `run`, `aggregate`) have **not** been started.

Snapshot: originally 2026-11-11 22:28 America/New_York (ET); branch-push
and hash refresh on 2026-11-12. Written so a new Grok Bot instance can
resume without the prior conversation. Re-check live state before acting:
other workers may have landed `fixup!` commits on `release/1.1` after
this file.

Do not merge `release/1.1` to `main` without Robert. Do not use
`/workspace/fix-wt` for 1.1 code work; that worktree tracks `main`.
The main-side pointer lives in `project/HANDOFF.md` and
`project/HANDOFF-20261112.md` on `main`.

Canonical docs on this branch: `project/plan/ROADMAP-1.1-PLAN.md` (order,
PIT plan, defaults) and `scripts/perf/pit.sh` (the driver). The older
beta1-suite notes live in `/workspace/fix-wt/project/HANDOFF-perf-20261111.md`
(the copy under this worktree is stale and is missing the ES 9 TLS finding).

---

## 1. Resume checklist

1. Read this file and `project/plan/ROADMAP-1.1-PLAN.md`.
2. Confirm weekly usage. At 90% or more: do not start new heavy work;
   `touch /workspace/perf-b1/STOP` and `/workspace/perf-pit-1.1/STOP` if
   those drivers are running, update this handoff, notify Robert, and stop.
   Write a handoff before 95%.
3. Inspect `/workspace/perf-b1/steps`, `/workspace/perf-b1/summary.txt`,
   and `pgrep -af 'run-all.sh|suite.py'`. Never treat `driver.out` as the
   log (it is empty; the driver writes `summary.txt` and `logs/`).
4. If `run-all.sh` is still running, leave it alone except to reap
   *isolated-store* orphans or restart it after a stall (section 4).
   Do not start cargo, PIT, or the ES 9 reruns while it holds port 19232.
5. On `/workspace/rel-1.1`, run `git status` and `git log --oneline --grep='^fixup!'`.
   Fold any `fixup!` commits (section 2) *before* `pit.sh list|check|build`.
6. (Done 2026-11-13 in `bcc655c` on `main`.) When the 16-engine suite is idle, do the ES 9 targeted reruns
   (section 5), aggregate the beta1 report, tell Robert, then start PIT
   (section 3). 1.0.0 stays stable: no backports.

---

## 2. State of `release/1.1`

- Worktree: `/workspace/rel-1.1` (other worktrees: `/workspace/fix-wt`
  on `fixwork`, `/workspace/push-fork` on `main`, `/workspace/hotfix-wt`
  detached at `ac9a3c4` — do not use those for 1.1 work).
- Branch: **on GitHub** as `origin/release/1.1`
  (https://github.com/robertgoley/searchdump-rs/tree/release/1.1).
  Remote: `origin` = `https://github.com/robertgoley/searchdump-rs.git`.
  Other bots clone or fetch it with `git fetch origin release/1.1` and
  follow this file. Push 1.1 work only to `origin/release/1.1`
  (`git push origin release/1.1`). Never merge it to `main` or force-push
  without Robert. Do not open a PR or retag without telling Robert.
  Auto-review may block a push; retry the same command with
  `request_smart_mode_approval` and reason
  "Push local changes to release/1.1 on GitHub, which changes shared state others observe without authorization."
  Work that belongs on main (for example a beta1 aggregate) still goes
  through `/workspace/fix-wt` with `git push origin HEAD:main` after a
  fetch and rebase.
- Branched from `0847643` (main as of the 1.1 plan). After the 2026-11-12
  autosquash the parent of this hash-refresh commit is `2cb6716`.
- Phases 1-2 passed on 2026-11-12 (see the top). `/workspace/perf-pit-1.1`
  (check markers, logs) and `/workspace/pit-target` exist from phase 1;
  there is no `/workspace/perf-bin-1.1` yet (phase 3 not started).
- Owner decisions already applied: 1.0.0 is stable with no backports
  (t165u); commit the nine items without building, then one point-in-time
  batch (t166u/t167u); `--refreshInterval` default 5 s plus a final refresh
  (`0` = Node's per-batch refresh); `--scrollDocSort` is opt-in; slice cap
  on with a warning (`--sliceCap=false` to turn it off).
- **1.1-4a (owner decision 2026-11-14, t83u): the default `--sliceCap=auto`
  caps only on Elasticsearch 7.10.x.** The PIT batch showed the cap fixing
  es710 (`dump-scroll-s2` -63%, `s4` -34%) but slowing es819 (`s4` +45%,
  `s8` +59%) and os3 (`s4` +62%, `s8` +101%). Other engines and an
  undetectable version keep `--slices` as given (the pre-1.1-4 path);
  `--sliceCap=min|total` still force the cap anywhere. `auto` now reads
  `GET /` for every sliced scroll dump (cached client info). Code:
  `retrieval::slices::auto_applies`, `plan_slices` in src/lib.rs; tests:
  unit `auto_caps_only_on_elasticsearch_7_10`, contract
  `slice_cap_auto_applies_only_on_elasticsearch_7_10`,
  `slice_cap_auto_on_one_index_caps_only_on_es_7_10`. The full PIT batch
  was not re-run; notes in CHANGELOG, RELEASE-NOTES-1.1.md, README Tuning,
  NODE-DIFFERENCES, scripts/perf/README.md and pit-1.1.md. A follow-up
  could re-measure es819/os3 `dump-scroll-s*` to confirm the return to the
  1.1-2 numbers.

### Commits after `0847643` (oldest first)

| Commit | Subject | PIT step? |
|---|---|---|
| `85856a4` | plan: 1.1 roadmap implementation order | no (docs) |
| `4cffbf4` | perf(1.1-5): blocking wait4 timer in measure.py | no (harness only; in effect for every PIT step because the harness always comes from HEAD) |
| `04e0af0` | bulk(1.1-2): refresh only the indices written | yes (step 1) |
| `d280836` | bulk(1.1-1): refresh every `--refreshInterval` seconds (default 5) | yes (step 2) |
| `87d1781` | retrieval(1.1-3): opt-in `--scrollDocSort` | yes (step 3) |
| `aee5dfb` | retrieval(1.1-4): cap scroll `--slices` to the shard count, `--sliceCap`, `--sliceField` | yes (step 4) |
| `7ebd7d0` | pipeline(1.1-9): gzip/zstd on a per-part compression thread | yes (step 5) |
| `99d5cb8` | bulk(1.1-6): encode the next bulk batch while one is in flight | yes (step 6) |
| `0e00d92` | multi(1.1-8): tokio::process children, Ctrl-C stops running children | yes (step 7) |
| `7910c9a` | multi(1.1-7): child-reported record counts, `--verifyCounts` | yes (step 8) |
| `6055414` | perf(1.1): point-in-time plan, resumable `pit.sh` | no (scripts/docs) |
| `64a8cfa` | docs: cold-start handoff for release/1.1 and the beta1 suite | no |
| `5e2a14c` | docs: draft 1.1 release notes | no |
| `206e1ab` | docs: README 1.1 flag notes (slices, `--verifyCounts`, release-notes link) | no |
| `2cb6716` | docs(1.1): update commit hashes after the earlier autosquash | no |
| (this file) | docs: refresh 1.1 hashes after the 2026-11-12 autosquash; branch is on origin/release/1.1 | no |

PIT base (step 0) is tag `v1.0.0-beta1` = `94e4afb`. `pit.sh list` is the
source of truth for hashes; they change after an autosquash. Hashes above are
after the 2026-11-12 autosquash (seven pending fixups folded into their
targets; tree identical to pre-squash `194f2f0`). Further `pit.sh check`
autosquash is a no-op unless new `fixup!` commits land.

### Fixup commits and autosquash (do this before the batch)

The Rust commits were written without `cargo check`. 1.1-6 (`99d5cb8`) and
1.1-8 (`0e00d92`) are the most likely to need small compile fixes. All
fixups present at the 2026-11-12 push were already folded (including
earlier `bf3cfa0` into 1.1-8). Concurrent workers may still add more
`fixup!` commits on this branch.

Before `pit.sh list|check|build`, and again if `check` or tests fail:

```bash
cd /workspace/rel-1.1
git log --oneline v1.0.0-beta1..HEAD
git log --oneline --grep='^fixup!' v1.0.0-beta1..HEAD
# If check/tests fail, commit the fix against the failing step:
#   git commit --fixup=<short-or-full-hash-of-the-item-commit>
# Then fold and push only to origin/release/1.1, never to main:
git rebase -i --autosquash v1.0.0-beta1
# Re-run list (hashes changed) and the failed phase.
scripts/perf/pit.sh list
```

If there are no `fixup!` commits, the autosquash is a no-op (that is the
state at the first push of this branch).

Do not squash across item boundaries. Do not fix up into docs-only
commits if the change belongs in a Rust item. Do not touch `.rs` files
except to land those compile/test fixes as `fixup!` commits.

---

## 3. Point-in-time (`pit.sh`) phases and exact commands

Start only when `pgrep -f run-all.sh` is empty, all 16
`/workspace/perf-b1/steps/*.done` exist, and the ES 9 reruns (section 5)
are finished — they share port 19232 and the isolated podman store.
Run niced from `/workspace/rel-1.1` at the head of `release/1.1`.
Nothing in PIT should write into `fix-wt`, `perf-b1`, or `perf-bin-beta1`.

Defaults (see `scripts/perf/pit.sh`): `PIT_STATE=/workspace/perf-pit-1.1`,
`PIT_BIN_ROOT=/workspace/perf-bin-1.1`, `PIT_SRC_ROOT=/workspace/pit-src`,
`PIT_TARGET=/workspace/pit-target`, `PIT_BASE=v1.0.0-beta1`,
`PIT_ENGINES="es710-plain es819-plain os3-plain"`, `PIT_REPS=3`,
`PIT_NODE=base`, `PIT_AB=1`, `PIT_TOOLCHAIN=1.91.1`, `PIT_PROFILE=release`,
`PIT_JOBS=2`, `PIT_NICE=10`. A `STOP` file in `PIT_STATE` halts `run`
between (build, engine) pairs. Markers make every phase resumable.

### PIT_ENV and PIT_REAP_* (copy from the beta1 driver)

`/workspace/perf-b1/env.sh` (source it via `PIT_ENV`; do not edit that
file). `pit.sh` overrides `PERF_BIN_DIR` and `PERF_HOST_ID` per step:

```
export PATH=/workspace/perf-b1/bin:$PATH
export XDG_RUNTIME_DIR=/tmp/xdg-runtime-perf-b1
mkdir -p "$XDG_RUNTIME_DIR"; chmod 700 "$XDG_RUNTIME_DIR"
export PATH=$HOME/.cargo/bin:$PATH
export NODE_ED_BIN=/workspace/node-ed/node_modules/.bin
export PERF_BIN_DIR=/workspace/perf-bin-beta1
export PERF_ENGINE_PORT=19232
export PERF_HOST_ID=grokbox-beta1
```

`/workspace/perf-b1/bin/podman` forces the isolated store
(`--root /home/box/.local/share/edrs-odt-podman --runroot /tmp/edrs-odt-podman/run`).
`CONTAINERS_STORAGE_CONF` is **not** honoured on this box.

Reap patterns, copied from `/workspace/perf-b1/run-all.sh`. They must
match only this isolated store and this driver's pasta netns — never the
shared store, never port 19222, never the uid 100999 OpenSearch process:

```
PIT_REAP_CONMON='conmon .*share/edrs-odt-podman/'
PIT_REAP_PASTA='pasta .*xdg-runtime-perf-b1/netns'
```

### Commands

```bash
cd /workspace/rel-1.1
export PATH="$HOME/.cargo/bin:$PATH"
# Fold fixups first (section 2), then:

scripts/perf/pit.sh list

# Phase 1 — cargo check --all-targets --locked at every Rust step (~25 min).
# On failure: fixup + autosquash, then rerun list + check.
scripts/perf/pit.sh check

# Phase 2 — tests and clippy at HEAD only (~30-40 min).
CARGO_TARGET_DIR=/workspace/rel-1.1/target nice -n 10 \
  cargo +1.91.1 test --locked
CARGO_TARGET_DIR=/workspace/rel-1.1/target nice -n 10 \
  cargo +1.91.1 clippy --all-targets --locked -- -D warnings
(cd scripts/perf && python3 -m unittest selftest)
# Fixes again as fixup! commits folded into the item commit, then re-check as needed.

# Phase 3 — release binaries into /workspace/perf-bin-1.1/<shorthash>/ (~1.5 h).
scripts/perf/pit.sh build

# Phase 4 — measure, engine by engine, all steps back to back (~6 h).
PIT_ENV=/workspace/perf-b1/env.sh \
  PIT_REAP_CONMON='conmon .*share/edrs-odt-podman/' \
  PIT_REAP_PASTA='pasta .*xdg-runtime-perf-b1/netns' \
  scripts/perf/pit.sh run

# Phase 5 — step-by-step table.
scripts/perf/pit.sh aggregate
# Report: /workspace/perf-pit-1.1/pit-report.md
```

`scripts/perf/pit.sh all` is `build && run && aggregate` only; it does
not run `check` or the HEAD tests. End to end about 8.5 h of box time.
Cheaper variants: `PIT_REPS=2` (~-1.5 h) or two engines (~-2 h). Pass
criteria and expected per-step effects are in
`project/plan/ROADMAP-1.1-PLAN.md`. Item checks the suite does not cover
(nodecmp for `--scrollDocSort`, golden `--refreshInterval=0`, IT with
`--sliceCap=false`, Ctrl-C of searchdump-multi, `--verifyCounts`) are
listed there and can wait until after phase 2.

---

## 4. Beta1 suite state (as of 2026-11-11 22:28 ET)

Driver: `/workspace/perf-b1/run-all.sh` (pid 2422403 at snapshot), cwd
`/workspace/fix-wt`, env `/workspace/perf-b1/env.sh`. Resumable via
`/workspace/perf-b1/steps/<engine>.done`. `touch /workspace/perf-b1/STOP`
halts it between engines. Binaries: `/workspace/perf-bin-beta1` (release
x86_64-gnu from GitHub Actions run 43077974616, plus locally built
edgen/edverify); hashes in `/workspace/perf-b1/bin-sha256.txt`.
`PERF_HOST_ID=grokbox-beta1`. `PERF_ENGINE_PORT=19232`.

`/workspace/perf-b1/driver.out` is **0 bytes** (redirected, but the
script logs only to `summary.txt` and `logs/`). Read those.

15 of 16 engines have a `.done` file. **os3-secure is in progress**
(started 22:22:30 ET, `suite.py --engines os3-secure`, result dir
`/workspace/fix-wt/results/perf/20261111T222230-2866421`). Earlier
OpenSearch secure engines took ~19–20 minutes, so a finish around
22:42–22:50 ET is reasonable, not a promise.

### Successful result dirs (use these, not the 1-second fast-fails)

| Engine | rc in .done | Result dir | Window (ET) |
|---|---|---|---|
| es710-plain | 0 | 20261111T133656-2100377 | 13:36:56–14:10:16 |
| es710-secure | 0 | 20261111T141016-2149931 | 14:10:16–14:36:29 |
| es717-plain | 0 | 20261111T143630-2201082 | 14:36:29–14:53:35 |
| es717-secure | 0 | 20261111T151838-2262122 | 15:18:36–15:39:53 |
| elasticsearch8-plain | 0 | 20261111T153954-2312312 | 15:39:53–15:57:10 |
| elasticsearch8-secure | 0 | 20261111T161226-2367612 | 16:12:24–17:50:38 |
| es819-plain | 0 | 20261111T175931-2422461 | 17:59:30–18:16:11 |
| es819-secure | 0 | 20261111T181614-2468942 | 18:16:13–18:33:55 |
| elasticsearch-plain | 1 | 20261111T183358-2515936 | 18:33:57–19:21:33 |
| elasticsearch-secure | 1 | 20261111T192136-2562801 | 19:21:35–20:48:20 |
| os13-plain | 0 | 20261111T204822-2615419 | 20:48:22–21:06:29 |
| os13-secure | 0 | 20261111T210632-2663982 | 21:06:32–21:26:24 |
| os2-plain | 0 | 20261111T212626-2715379 | 21:26:26–21:44:34 |
| os2-secure | 0 | 20261111T214436-2762705 | 21:44:36–22:03:40 |
| os3-plain | 0 | 20261111T220343-2814524 | 22:03:42–22:22:28 |
| os3-secure | (running) | 20261111T222230-2866421 | started 22:22:30 |

All result paths are under `/workspace/fix-wt/results/perf/<dir>/`.
elasticsearch-plain and elasticsearch-secure were marked done because
`summary.md` contains `PASS`, even though `rc=1`. They still need the
targeted reruns in section 5. Do not rerun a full engine if only those
cases failed.

`.done` plus `rc=0` in `summary.txt` is not proof the cases ran. A leftover
engine on 19232 produces a failed `combined.json` in about a second
(`FAIL: setup/seed interrupted`, no wall times). Before aggregating,
count PASS/SKIP rows in that engine's `summary.md` / `combined.json`, and
confirm the directory in the `.done` file is the successful retry (later
timestamp), not the fast-fail.

Known stalls today: after es717-plain a leftover held 19232 and the next
13 engines fast-failed; the watch restarted from es717-secure at 15:18.
elasticsearch8-secure fast-failed at 15:57 (orphan from es8-plain);
restarted at 16:12. es819-plain fast-failed at 17:50 (orphan from
es8-secure); the driver was patched with the orphan reaper and port
check (local `/workspace/perf-b1/run-all.sh` only) and restarted at
17:59:30. That run-all.sh fix is outside the repo.

Restart after a stall, only if the driver has exited and 19232 is free
(after reaping isolated-store orphans with the same patterns as
run-all.sh):

```bash
cd /workspace/perf-b1 && setsid nohup ./run-all.sh > driver.out 2>&1 < /dev/null &
```

When all 16 are really done (and after the ES 9 reruns):

```bash
cd /workspace/fix-wt && python3 scripts/perf/aggregate.py \
  --title "v1.0.0-beta1 L0 perf suite" \
  --old-baselines 'scripts/perf/baselines/grok-bot-vm-417583633-box-L0-*.json' \
  $(sed -n 's/.*dir=//p' /workspace/perf-b1/steps/*.done) \
  > scripts/perf/results/v1.0.0-beta1-L0.md
```

If a rerun produced a new result dir, pass that dir instead of (or in
addition to, as aggregate.py requires) the partial one — use the dirs
that actually contain the passing cases. Commit the report on `fixwork`
and push to `main` only after a fetch and rebase. Then summarize for Robert.

---

## 5. ES 9 edge reruns (done)

**Done on `main` in `bcc655c` (2026-11-13 00:13 America/New_York).** The isolated reruns of the
three ES 9.1.2 failures all passed: elasticsearch-plain `dump-edge` / `restore-edge` (the seeder
stalls) and elasticsearch-secure `dump-zstd` (the edverify scroll timeout). The re-aggregated
`scripts/perf/results/v1.0.0-beta1-L0.md` on `main` has 576 PASS, 0 FAIL and the same 2.01x
median versus Node, so the beta1 baseline is complete for every engine. Listing these reruns as
pending or deferred on 2026-11-14 was a tracking error; nothing is left to run here. The
commands that were used are in `project/HANDOFF-20261112.md` on `main`.

### 5a. 1.1-10 small-heap `searchdump-multi` defaults (2026-11-14)

Reproduced on the box (bitnamilegacy ES 7.10.2 and 8.6.1 die with `OutOfMemoryError` at a 256m heap
under the old `--limit=5000 --maxResponseMb=0`; ES 8.19.23 returns HTTP 429 from the parent
breaker) and fixed on `release/1.1` only (see `project/plan/ROADMAP-1.1-PLAN.md` 1.1-10 and
`results/local/1.1-10.md`). For the kvmsvr1 RC run: `searchdump-multi` data runs now page 1000
documents (fewer for documents over 16 KB stored) with the 256 MB response cap, and loads split
bulks at 16 MB; any perf step that
times `searchdump-multi` without `-- --limit=...` measures the new default. Whether the same change
should also go to `main` (the 1.0 line) is the owner's decision.
The low-memory regression set is `python3 scripts/load/lowheap.py --regression` (L-818, about 16 minutes on
the box; `scripts/load/README.md`); run it with the RC binaries, it needs only about 2 GB free RAM.

---

## 6. Routine behaviour

Name: **searchdump-rs beta1 perf suite watch**. Folder:
`searchdump-rs-beta1-perf-suite-watch`. Schedule: hourly, flexible.

Each hourly run should:

1. Check weekly usage. At **90% or more**: `touch /workspace/perf-b1/STOP`
   (and `/workspace/perf-pit-1.1/STOP` if PIT is running), write or update
   this handoff (and the beta1 perf handoff if needed), notify Robert, and
   **delete the routine**. Always write a handoff before 95%.
2. If the driver has died or fast-failed: reap orphans in the *isolated*
   store only (the `PIT_REAP_*` / run-all.sh patterns), confirm port 19232
   is free, and restart `run-all.sh` as in section 4. Do not restart over
   a still-running driver.
3. Never touch the uid 100999 OpenSearch java process, port 19222, the
   shared podman store, or the leftover shared-store conmon (historically
   pid 1507462, name `it-os-sec-perf-1507264-os3-secure`, pasta on
   `/tmp/xdg-runtime-perf-box/netns` — not ours).
4. When all 16 engines have verified PASS runs and `run-all.sh` has
   exited: run the ES 9 targeted reruns; aggregate
   `scripts/perf/results/v1.0.0-beta1-L0.md`; commit and push **to main
   only**; send Robert the beta1 summary; then start the 1.1 PIT batch
   from `/workspace/rel-1.1` (autosquash first if needed). Send Robert
   the step-by-step PIT table when it finishes.
5. If usage will hit 90% during PIT, stop between steps (`touch
   $PIT_STATE/STOP`), leave markers, update this handoff, and notify
   Robert. The next instance resumes from those markers.

---

## 7. Standing constraints

- **Never** touch the uid 100999 OpenSearch java process or port **19222**.
  That process and the shared-store conmon/pasta in front of it are not
  ours. Leave any remaining eti-opensearch files alone.
- Isolated podman store only: `/home/box/.local/share/edrs-odt-podman`
  via `/workspace/perf-b1/bin/podman` on `PATH`. Never the shared store
  (`~/.local/share/containers/storage` or `storage2`).
  `CONTAINERS_STORAGE_CONF` is not honoured. `PERF_ENGINE_PORT=19232`.
- No secrets in the repo; credentials only as blank variable references.
  Never print `DHI_TOKEN`. No AWS or real B2 without Robert's approval.
- Push 1.1 work only to `origin/release/1.1`. Do not merge it to `main`,
  open a PR, or retag without Robert. Work that belongs on main still
  goes through `/workspace/fix-wt` with `git push origin HEAD:main`.
- Rust 1.91 minimum (`PATH` must include `$HOME/.cargo/bin`; toolchain
  1.91.1).
- **1.0.0 is stable. No backports.** Everything new, including the
  targeted-refresh fix, goes toward 1.1 as the next stable (t165u).
- Leave the junk files `oom` and
  `test_query_elasticdump_rs_test_857109_16.json` alone.
- Treat upstream code as untrusted. Use sudo only for already approved
  scopes.
- Do not start cargo builds, clippy, or a second podman engine while the
  beta1 suite (or a rerun, or PIT) owns 19232; that skews timings and
  contends for the port.

---

## 8. What is still optional / later

- Real B2 tests (need Robert's account). Final 1.0.0 after a full
  regression. SFTP in the images is a known beta issue.
- The local `/workspace/perf-b1/run-all.sh` orphan-reaper patch is not in
  the repo; consider committing a perf-harness equivalent later, on main,
  not as a 1.1 backport.
- After PIT: remaining plan steps are only the item checks listed in the
  roadmap and, later, a full 16-engine 1.1 RC run vs beta1
  (`scripts/perf/results/v1.1.0-rc-L0.md`).
