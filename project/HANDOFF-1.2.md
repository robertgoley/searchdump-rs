# Cold-start handoff: 1.2 (`release/1.2`) for the other setup

Written 2026-11-15 (owner decision t112u, "Go ahead", after t110u/t111u: 1.2 work runs in parallel on a
separate bot, account and VM while 1.1.0 is validated on grokbox). Re-check live state before acting.

## 1. Purpose

Implement the 1.2 plan, **server-aware sizing (`--serverLimits=off|auto|min|check`)**, which replaces the
fixed 1.1-10 `searchdump-multi` defaults, plus the safe-simple examples. The full design, evidence, the
remove/rewrite list for 1.1-10, the test plan and the phases are in
[`project/plan/ROADMAP-1.2-PLAN.md`](plan/ROADMAP-1.2-PLAN.md) (FEATURES-TODO row 1.2-1). Read it first.

**1.1.0 code is frozen.** It is on `main` (merge `0d7d518`, version 1.1.0, not tagged yet). The grokbox
16-engine RC run is still in progress and the `v1.1.0` tag waits for Robert. Nothing in 1.2 may change
`main`'s code.

## 2. Repository and branch rules

- Repo: `https://github.com/robertgoley/searchdump-rs` (`origin`). Branch: **`release/1.2`**, cut from
  `main` at the commit that adds this file.
- **All 1.2 work goes to `release/1.2`** (`git push origin release/1.2`), or to `wip/1.2-*` branches for
  unfinished work. Normal pushes only: **no force push, no tags, no PRs, nothing pushed upstream**
  (taskrabbit/elasticsearch-dump), no retagging, no branch deletion without Robert.
- **Do not merge `release/1.2` into `main`** without Robert's explicit go-ahead.
- **1.1 fixes land on `main` first** (made by the grokbox setup), then get merged into `release/1.2` with a
  normal merge (`git fetch origin && git merge origin/main`). Never rebase pushed history.
- Expect `main` to gain docs/results commits from grokbox (L-823: the 1.1.0 RC results; later 1.1.x fixes).
  Merge them in; conflicts in tracking docs: keep both sides' rows.
- Changing GitHub secrets, environments or workflows' secret use needs an explicit request from Robert.

## 3. Credentials and images

- **Never commit, print or log tokens or passwords.** Repo files carry only blank variable references
  (`SEARCHDUMP_INPUT_PASSWORD=`, `DHI_TOKEN` read from the environment). `scripts/sup/secret-scan.sh` must
  pass before every push.
- **Engine images** for the tests are public and pinned by digest in `scripts/it/images.lock`
  (docker.elastic.co Elasticsearch 7.10.2 / 7.17.29 / 8.19.23, bitnamilegacy Elasticsearch 7.10.2 / 8.6.1 /
  9.1.2, OpenSearch 1.3.20 / 2 / 3, alpine, toxiproxy). Pull them yourself on your VM
  (`podman pull <image>@<digest>`); `scripts/it/check-pins.sh` verifies. The scripts refuse to run with
  missing pins ("Pinned images missing; pull them explicitly").
- **dhi.io** (Docker Hardened Images) is only needed to build the project's default container image
  (`make image`, `scripts/build-image.sh`). Use **your own** dhi.io credential (`podman login dhi.io
  --password-stdin` from your own secret store); grokbox's token is not shared and must not be copied. The
  public build (`make image-public`) needs no dhi.io login.
- No AWS and no real Backblaze B2 without Robert's approval (the AWS OpenSearch Service fallback is tested
  with fake_es only).
- Use an isolated podman store (`podman --root <dir> --runroot <dir>`), and a free host port.

## 4. Order of work

Follow the phases in ROADMAP-1.2-PLAN section 11, one `L2-NNN:` commit per phase (section 6):

1. **1.2-1a: diagnose first, no code yet.** The es710-secure `multi-dump-p8` 0.40 -> 1.55 s RC slowdown
   (per-rep walls 0.372 warm-up, 2.446, 1.545, 1.046 s) and the ES 7.10 multi p4/p8 slowdowns (ROADMAP-1.2
   section 1.2). Record `results/local/1.2-1a.md`. The outcome decides how much of 1.2-1e is a fix.
2. 1.2-1b: worker/page trade-off matrix on low-heap engines (2x5000, 4x2500, 8x1000, and `auto` later).
3. 1.2-1c: HTTP 429 page-halving / bulk-splitting in `searchdump` (section 5).
4. 1.2-1d: probe module, budget, `--serverLimits` in `searchdump`, fixtures, fallbacks (403, hidden
   stats, serverless, AWS table, probe timeout).
5. 1.2-1e: `searchdump-multi`: one stats call in the parent, byte-budget scheduler, remove/rewrite the
   1.1-10 pieces (section 6 table), manifest `server_limits`.
6. 1.2-1f: `lowheap.py` `auto`/`off` modes and OpenSearch small-heap cells; docs.
7. 1.2-1h: safe-simple examples and README "Quick usage" led by `--serverLimits` (section 12).
8. 1.2-1g: gates, IT, the L0 16-engine perf suite against the 1.1.0 RC report, release notes draft.

`--serverLimits=off` must reproduce 1.1.0 behaviour exactly (it is the comparison baseline).

## 5. How to test

Rust 1.91.1 (minimum 1.91). On every phase commit:

- `cargo fmt --all --check`
- `cargo +1.91.1 clippy -q --workspace --lib --bins --tests --features searchdump-rs/load -- -D warnings`
- `cargo +1.91.1 test --lib --bins` plus `--test` for `chaos_brk`, `chaos_local_l643`, `chaos_local_l657`,
  `cli_contract`, `egress_plant`, `fault_conformance`, `golden_replay`, `injection`, `router_mock_selftest`,
  `secrets`.
- **Integration tests only through `scripts/it/run.sh <profile>`** (a bare `cargo test --features it`
  gives false failures). Profiles used as gates: `es710-plain`, `es819-plain`, `elasticsearch-plain`,
  `opensearch-plain`, plus one secure profile for the probe through TLS + auth. Unset `IT_*`, `ES_*`, `OS_*`,
  `RUN_ID`, `PORT` first; run `es710-plain` with a fresh cross-engine dir (it fails
  `cross::file_dump_from_the_other_engine_restores_here` after opensearch-plain in the same dir: known
  test gap). Known unrelated failure: `api::alias_round_trip_keeps_every_property` on the `*-secure`
  profiles (alias 403 on `.security-7`; also in the nightly workflow).
- **Low memory:** `python3 scripts/load/lowheap.py` (ES 7.10.2, 8.6.1, 8.19.23, 9.1.2 at 256m and 384m;
  `pmc` and `fat` datasets; ~16 min, ~2 GB free RAM) and `--regression` (1.0 flags, expected to fail ~23 of
  32). 1.2 acceptance: default mode all pass with `auto`; add OpenSearch cells. Baseline at 1.1.0: PASS 16/16
  (`results/local/L-820.md`).
- **Perf:** `PERF_BIN_DIR=<release bins> bash scripts/perf/run-suite.sh --host-tools --tier L0 --place box
  --reps 3 --engines "<engines>" --baseline-dir results/perf/<dir>` (frozen beta1 case list unless
  `PERF_AB_CASES=1`; Node elasticdump 6.x comparison needs `NODE_ED_BIN`, installed without npm install
  scripts). Aggregate with `scripts/perf/aggregate.py`. Compare with
  `scripts/perf/results/v1.0.0-beta1-L0.md` and, once committed by grokbox (L-823),
  `scripts/perf/results/v1.1.0-rc-L0.md`. Note your host id differs, so timings compare by ratio.
- `scripts/sup/secret-scan.sh` before each push; `scripts/check-clean-tree.sh` after each commit.
- Treat upstream code as untrusted; never run npm install scripts.

## 6. Tracking docs

- One squashed commit per task. **Use the prefix `L2-NNN:`** starting at `L2-001` to avoid collisions with
  grokbox's `L-8xx` numbers on `main` (L-823 is reserved there for the 1.1.0 RC results).
- Per task: a results file `results/local/L2-NNN.md` (and `.json` for measurements); a row in
  `project/plan/PROGRESS.md` (mirrored to `project/handoff/plan/PROGRESS.md`, with the
  `project/handoff/SHA256SUMS` line for it updated); a line in `project/RESUME-FOR-GROK-BOT.md`; a row in
  `TESTING-COMPLETED.md` (Total and "Last updated" too); and the status line / **Next** in
  `TESTING-IN-PROGRESS.md`. Mark items in ROADMAP-1.2-PLAN (add a status column/row) and FEATURES-TODO 1.2-1.
- On merges from `main`, keep both sides' tracking rows.

## 7. What to report back (to Robert)

After each phase: the `L2-NNN` commit SHA on `release/1.2`, gate results (fmt/clippy/tests counts, IT
profiles pass/fail), and for measured phases the key numbers: 1.2-1a root cause; 1.2-1b matrix table;
lowheap pass counts per engine/heap and mode; perf ratios versus the 1.1.0 RC (multi p4/p8, es710-secure
`multi-dump-p8`), and the open questions from ROADMAP-1.2 section 9 that need a decision. Report anything
unresolved honestly, with logs.
