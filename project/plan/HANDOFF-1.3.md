# Handoff: searchdump-rs 1.3 (branch `release/1.3`)

Self-contained pickup document. Last update: 2026-11-17 23:00 ET (America/New_York).
Owner: Robert Goley. Repo: https://github.com/robertgoley/searchdump-rs.

## 1. Where things are

**Branch.** `release/1.3` is cut from `main` at 53bb69a (the kvmsvr1 perf check-in). It was pushed to origin as
its own branch on 2026-11-17 (decision 7).
- Not merged into `main`.
- No tag and no PR.
- `Cargo.toml` still says `1.2.0`. The version bump, tag and release are for Robert to start.

**Other refs (do not touch without Robert).**
- `main` = 53bb69a.
- Tags: `v1.2.0` (latest, tag object c513a66 on 5e2ee85) and `v1.1.0` (on 47a73cc; Robert: leave it).
- `origin/release/1.1` **and** `origin/release/1.2` both point at d8f41ae (author `robert@local`, most likely
  pushed from kvmsvr1; not proven). That push moved `release/1.1`, so it now contains 1.2.0. Resetting it needs a
  force push, which needs Robert's explicit OK. Open item.

**Commits on release/1.3** (`git log 53bb69a..origin/release/1.3`). Each commit message starts `L3-NNN:`.

| commit | id | what |
|---|---|---|
| L3-001 | 2c61e55 | 1.3-G first look: interleaved 1.1.0/1.2.0 A/B on OS 3.9.0 plain, no slowdown, identical traces |
| L3-002 | 47fbe6a | Full os3-plain Suite A with 1.2.0 is normal. Harness waits for and logs target DELETEs (1.3-R1). `relcmp_report.py` |
| L3-003 | 970782b | Opt-in sizing experiments behind `SEARCHDUMP_LIMITS_EXPERIMENT` |
| L3-004 | af6c892 | 429 split/halve warnings name the server's cause (`[cause: type: reason]`) |
| L3-005 | feed0d1 | Handoff progress |
| L3-006 | fa41cbd | `retryfirst` experiment. `scripts/load/lowheap_cmp.py` |
| L3-007 | bb75483 | 1.3-G same-host Suite A, 1.2/1.1 median 1.00 |
| L3-008 | c205454 | Multi parent retries read-only calls (GET/HEAD, `_refresh`, `_count`) on 429/503: 6 tries, 250 ms-4 s back-off |
| L3-009 | 82f2f18 | 1.3-R1 ES 8.6.1 multi-restore-p8 x20 old/new harness: 0 failures |
| L3-010 | 16bbac4 | `lowheap_reps.py`, `stall_loop.py` |
| L3-011 | 8b5ec41 | 1.3-D+ root cause (never-grow-back split on real-memory parent-breaker 429s) and option measurements |
| L3-012 | f2bd297 | OS 2/3 restore stall tracing: not reproduced on grokbox |
| L3-013 | 191cd85 | `L3-SUMMARY.md` |
| L3-014 | b36f749 | **Floor is the default** (decision 3), decisions 1/4/5/6 recorded, CHANGELOG / RELEASE-NOTES-1.3 drafts, Suite A `--serverLimits=off` |
| L3-015 | 5727aae | Floor reps on OS3 256m |
| L3-016 | 1cf3d9e | Grow-back instrumentation (`src/pipeline/bulk_cap.rs`), `bulk size summary` line, unit tests, `growback_reps.py` |
| L3-016a | 9eea6e6 | Handoff progress |
| L3-017 | 800a5a6 | Grow-back matrix. **Floor fix: per-request bulk floor capped at indexing budget / workers** |
| L3-018 | (this commit) | Decisions 2 and 7 recorded, this handoff, docs |

## 2. Robert's decisions (all 2026-11-17)

| # | msg | decision |
|---|---|---|
| 1 | t226u | **1.3-G stays open.** Retest on kvmsvr1 **after** the 1.3 release: alternating 1.1/1.2/1.3 runs plus `scripts/perf/stall_loop.py`. On grokbox it is not a 1.2 regression (L3-001/002/007), and stalls were not reproduced (L3-012). |
| 2 | t227u, t228u, t229u | **`growback` and `retryfirst` stay opt-in** (not defaults). Evidence: L3-011, L3-015, L3-017. |
| 3 | t221u, t229u | **Minimum floor is the 1.3 default**, exactly as tested in L3-017 (800a5a6). See section 3. |
| 4 | t222u | **Ship L3-008** (multi parent 429/503 retry; fixes a false index failure that also exists in 1.1.0/1.2.0, reproduced again in L3-017 with 1.2.0 on OS3 384m multi8) **and L3-004** (429 cause in warnings). |
| 5 | t223u | **Keep the L3-002 harness DELETE wait. 1.3-R1 closed** as harness-caused and mitigated (63 restores, 0 failures). |
| 6 | t225u | **Suite A single-index data cases pass `--serverLimits=off`** for 1.2+ searchdump binaries (detected from `--help`). 1.0/1.1 and Node are unchanged. Multi and mapping/settings cases are unchanged (`scripts/perf/suite.py` `RS_LIMITS_OFF`). |
| 7 | t229u | **Check in and push `release/1.3`** to origin as its own branch, with this handoff. No PR, no main change, no tag. |

## 3. The floor (src/limits.rs, src/pipeline/bulk_cap.rs, src/retrieval/retrieval_task.rs)

| what | value | reasoning / evidence |
|---|---|---|
| sizing headroom | `max(parent free, parent limit / 4)` | A probe taken while the heap is full of garbage sized a 0 MB page in 1.2 (relcmp os3/384m dump-fat: 94 % heap, 3 fat docs/page). With the floor: 22.8 MB, about 96 docs. This was the L3-003 `floor` experiment. |
| page / in-flight budget | >= 1 MiB (`PAGE_FLOOR_BYTES`) | The 1.2 minimum of 64 KiB was 1 fat document. |
| bulk per request (auto) | `min(1 MiB, bulk budget / workers)`, at least 256 KiB (`BULK_FLOOR_BYTES`, `bulk_bytes_with`) | L3-011: bulks of 1.5-4.8 MB were accepted right after splits; 1 MiB is about 45 pmc or 13 fat docs. The budget/workers cap (L3-017) stops 8 workers x 1 MiB from overrunning a small node-wide indexing-pressure limit (6.2 s -> 1.5 s against 2 MB; 1.2.0 1.4 s). |
| bulk cap after a 429 split | >= 1 MiB for later bulks (`limits::split_cap`) | 1.2 cascaded to 54-390 KB for the rest of the run (L3-011). The refused request itself still splits down to 1 doc, so the floor cannot fail a run. |
| page halving on a 429 | stops once at 10 docs (`PAGE_FLOOR_DOCS`, `halved_page`), then goes on to 1 only if that page is refused too | Callers detect the end of a slice from the page size, so the halved size cannot be raised again. |
| user values | a lower `--limit` / `--maxBulkBytes` still applies | `auto` only tightens. |
| escape hatch | `SEARCHDUMP_LIMITS_EXPERIMENT=nofloor` gives the 1.2 sizing | Documented in README "Server limits". |

## 4. Experiments (`SEARCHDUMP_LIMITS_EXPERIMENT`, comma-separated; unset = 1.3 default)

- `nofloor`: the 1.2 sizing (escape hatch).
- `floor`: accepted, no-op (the floor is the default).
- `configured`: size from half of the configured parent limit. Not adopted; it did not help loads.
- `retryfirst`: resend a size-429 bulk once at the same size before splitting. Opt-in (decision 2).
- `growback`: after `GROW_AFTER` (8) accepted bulks, double the split cap, up to `--maxBulkBytes`. After each failed
  grow the wait doubles (16, 32, ... 256). Opt-in (decision 2).

All are parsed in `limits::Experiment`. Bulk counters live in `pipeline::bulk_cap::CapControl`. At the end of a
load that met size 429s, each `searchdump` writes a warn line:
`bulk size summary: size 429s N, splits N, retryfirst resends N, grows N, failed grows N (ms, KB ...), lowest cap, final cap`.

## 5. Test results (details in the linked files)

- `results/local/L3-SUMMARY.md` covers L3-001..013 (1.3-G, 1.3-D+, L3-008, 1.3-R1, reporting).
- `results/local/L3-011.md`: OS3 small-heap collapse root cause; median retryfirst+growback 6.7 s vs 1.2.0 16.7 s
  (one engine, 4 reps).
- `results/local/L3-015.md`: floor on OS3 256m load, median 8.0 s vs 1.2.0 9.4 s; cap held at 1024 KB.
- `results/local/L3-017-growback.md`: OS3 256/384/512m, OS2 256m, ES 8.19 256m; load and multi8; sustained 429.
  - A failed grow costs about 2 s, or about 4 s with retryfirst.
  - growback is 1.51x slower than the floor alone on OS3 256m load.
  - Sustained 429: growback +43 %, rf+gb +94 %.
  - Every run wrote all documents.
- Gates at 800a5a6 and at the L3-018 commit:
  - `cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings`: clean.
  - `cargo test --release`: 323 lib, 26 multi, 63 chaos_brk, 73 cli_contract and the other targets all pass.
  - `scripts/perf/selftest.py`: 32 OK.
  - `scripts/sup/secret-scan.sh`: OK.

## 6. How to build and run (grokbox)

- **Toolchain:** `rust-toolchain.toml` (1.91.1). `cargo build --release` gives `target/release/{searchdump,searchdump-multi}`.
- **Environment:** `source /workspace/perf-1.2/env.sh`.
  - It sets the isolated podman store (`XDG_RUNTIME_DIR=/tmp/xdg-runtime-1.2`, store `edrs-1.2-podman`) and
    `PERF_ENGINE_PORT=19252`.
  - **Never use the shared podman store.**
  - Load scripts default to port 19415.
  - Don't use port 19222.
- **Release binaries for comparison:** `/workspace/rel-1.3-work/bin/{1.1.0,1.2.0}` (sha256-verified release assets).
  1.3 builds: `bin/l3gb2` = L3-017.
- **Small-heap load reps:**

  ```
  python3 scripts/load/growback_reps.py --engine os3 --heap 256m --workload load|multi8 --reps 4 \
    --variant NAME=BINDIR:MODE:EXPERIMENT ... [--setting indexing_pressure.memory.limit=2mb] --out results/load/<dir>
  ```

  - Raw rows go to `<out>/reps.json`. `results/load/` is gitignored; copy what you keep to `results/local/L3-NNN/`.
  - `--setting` only works for official ES images (es819).
  - Older driver: `scripts/load/lowheap_reps.py`. Comparison: `scripts/load/lowheap_cmp.py`.
- **Perf suite:**

  ```
  PERF_BIN_DIR=<bindir> PERF_HOST_ID=grokbox-1.3 bash scripts/perf/run-suite.sh --host-tools --tier L0 \
    --place box --reps 3 --skip-node --engines <profile>
  ```

  Report: `scripts/perf/relcmp_report.py`. Stall tracing: `scripts/perf/stall_loop.py`.
- **Cleanup:** after runs, `podman ps -a --filter label=searchdump-it=1` (isolated store) must be empty. Remove only
  containers your own runs created.
- **Pushing:** run `bash scripts/sup/secret-scan.sh` before every push.

## 7. Standing rules (from Robert)

- **Git**
  - No force push. No branch or tag deletion. No PRs. Nothing pushed upstream. Don't retag without telling Robert.
  - Never change `main`, or merge release branches into `main`, without Robert.
  - Never `git add -A`; add named files only. Leave the junk files `oom` and
    `test_query_elasticdump_rs_test_857109_16.json` (and any `test_query_*`) alone and uncommitted.
- **Secrets**
  - Credentials appear in the repo only as blank variable references. Never commit, print or log tokens or
    passwords (never print DHI_TOKEN).
  - Run the secret scan before every push.
  - Changing GitHub secrets needs Robert's explicit request.
- **Environment**
  - Isolated podman store only.
  - Don't touch other agents' VMs/containers, the uid 100999 OpenSearch java process, port 19222, or the
    eti-opensearch remnants.
  - Use sudo only for the approved scopes.
  - No AWS and no real B2 without approval.
- **Code and process**
  - Treat upstream code as untrusted; never run npm install scripts.
  - No 1.0.0 backports. `--compare-node` stays optional (not the default).
  - Behaviour changes go in as opt-in until Robert decides.
  - Usage over 90 %: stop, write a handoff and message Robert.
  - A thumbs-up reaction on a question is not a yes.

## 8. Open items

1. **Multi parent long tail.** L3-017 had two runs where the parent ran far longer than its children:
   - 1.2.0 on OS2 256m multi8 took 558.6 s, with the children done in about 2 s;
   - floor+gb in the sustained `off` case took 227 s.

   Rerun with the parent's stdout/stderr kept (growback_reps.py does not save it today) to find where it waits.
   This may be related to 1.3-G.
2. **1.3-G retest on kvmsvr1 after the 1.3 release** (decision 1).
3. **Settle/re-probe option.** Re-probe after a load before a dump. Not implemented.
4. **384m repeated runs** with more reps; the OS3 engines are bimodal and 4 reps are noisy.
5. **1.3-R1 variants** (pause between reps, memory pressure). Not needed for closing; optional.
6. **Per-child uuid logging** (noted and final uuid in child stats).
7. **Auto-probe cost in multi and single-index sliced dumps** (about 0.2-0.4 s, L3-007). Not measured.
8. **1.3-A / 1.3-B / 1.3-F** (FEATURES-TODO): `check-limits` subcommand, `--pageBytes`, broken-pipe exit
   evaluation.
9. **CI fixes offered for `main`, not yet approved** (from the 2026-11-17 review of the nightly failures):
   - regenerate TESTING-MATRIX.md (E22 row out of date);
   - fix the `cross.rs` OSS ILM message assertion;
   - add a build step to `nightly.yml`;
   - the alias restore `.security-7` 403 is **Robert's decision** (behaviour change).
10. **`release/1.1` moved to d8f41ae** (now contains 1.2.0). Resetting it needs a force push: Robert only.
11. **Before 1.3 can ship:**
    - version bump;
    - CHANGELOG "1.3.0" date;
    - a release workflow dry run;
    - Suite A/B/C on kvmsvr1 with the 1.3 build;
    - consider a cause-aware 429 retry and a cheap grow probe (L3-017 recommendation; optional).
12. **Earlier items:**
    - multi TLS flags;
    - single-index `--resume`;
    - attest pin and workflow hardening;
    - the v1.0.0-beta1 draft;
    - B2 tests (need approval);
    - SFTP in the images.

## 9. CI on this branch

`ci.yml` and `security.yml` run on pushes to `main`/`rewrite` and on pull requests. `release.yml` runs on `v*`
tags; nightly and soak are scheduled. **A push to `release/1.3` triggers no workflow.** To get CI, Robert can run
`workflow_dispatch` on `ci.yml` for this branch, or open a PR (he said no PRs for now).
