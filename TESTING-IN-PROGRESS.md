# Testing in progress

The task plan delegated to the other agent (Grok 4.7 through OpenCode, using [`project/handoff/HANDOFF-GROK-OPENCODE.md`](project/handoff/HANDOFF-GROK-OPENCODE.md)). Everything below is assigned to it; it commits each task to `main` as `L-NNN: ...` and adds a row to [`project/plan/PROGRESS.md`](project/plan/PROGRESS.md). Finished tasks move to [TESTING-COMPLETED.md](TESTING-COMPLETED.md). Tests outside this plan are in [TESTING-TODO.md](TESTING-TODO.md).

Status as of 2026-11-14: work through L-808m (2026-11-09) is recorded in [TESTING-COMPLETED.md](TESTING-COMPLETED.md) and in the dated note in [`project/plan/PROGRESS.md`](project/plan/PROGRESS.md). Later work on this tree includes L-809 (OpenDAL storage), the searchdump-rs rename, 1.0.0-beta1/1.0.0, and the owner's 2026-11-14 decisions: L-645b (`--httpAuthFile` wins; done), L-810i (OpenSSH client in the images; CVEs accepted 2026-11-14, merged), release/1.1 1.1-4a (slice cap only on ES 7.10; done on release/1.1), L-804a (exit-code review; done, proposals await the owner). L-808n to L-808r (restore CPU profile and fixes; CPU about half, level with M6; done). **Still open from this plan: L-703** (seccomp/Landlock). Owner decision item D-1 is still not started. Deferred for later owner review (2026-11-14): see the table in section 1. **Next:** L-804b (exit-code proposals P1-P5, owner approval t86u), then L-809 (300-index campaign, approved t87u).

Who: impl = bounded code card, runner = scripted campaign, lead = supervised session for security-sensitive work, review = independent read-only review of every change.

## 1. Owner decision items

| Item | State | Who | Note |
|---|---|---|---|
| D-1 red-index partial results | not started | impl | Opt-in flag (suggested `--allowPartialResults`); refuse by default, warn with failed-shard count when allowed. Tests for both paths. |
| D-2 gzip default | done (D-2) | impl | Confirm gzip stays the default and zstd is opt-in in code, help and docs; add a test if missing. |
| D-3 Alpine Dockerfile default | done, changed by user decision (L-808b) | impl, runner | The user chose glibc + mimalloc as the default: `make image` / `scripts/build-image.sh` build on `dhi.io/static:20250419-glibc-debian13` (no shell); the dhi.io Alpine `-dev` image stays as `make image-dhi-alpine` / `--variant alpine` (`:<ver>-alpine`). CVE scan re-run: grype 5 High (glibc, libgcc-s; no fix), trivy 0 HIGH, the user accepted the 5 as known and open (L-808d): `make scan` ignores exactly those ids while their fix state is `not-fixed`/`wont-fix` (`docker/grype-ignore.yaml`) and passes on the default. |

Deferred by the owner on 2026-11-14 (t83u), kept for later review:

| Item | State | Note |
|---|---|---|
| Q5 `--filterManaged` skip line | deferred | The "skipped managed policy" line only shows under `--debug`; make it an info line? |
| Q6 ES 7.10 managed policies | deferred | ES 7.10 built-ins and `.monitoring-8-ilm-policy` lack the `managed` marker and are copied; skip by name? |
| ES 9.1.2 reruns | deferred, to be done later | Earlier failures traced to the harness stalling. |
| 1.1-8 flaky ES 7.10 rep | skipped for now | dump-split-gzip rep2 edverify diff (34 PASS / 1 FAIL). |
| policy_round_trip flake, `--ca` script test, B2/AWS real tests, dhi.io token rotation | skipped for now | Need owner input or accounts. |
| L-804a P1-P6 | awaiting owner | results/local/L-804a.md. |
| L-808n next candidates | done 2026-11-14 (t85u): L-808o, L-808p, L-808q; restore CPU about half of pre-L-808n; RSS still ~2.4x M6 (results/local/L-808o.md) | results/local/L-808n.md (single parse, buffer reuse / RSS, NDJSON splitter). |

D-4 (per-direction credential flags, L-645) and D-5 (leftover `.part` warning, L-648) are done.

## 2. Runtime supply chain (remaining, A7)

| Item | Req | Who | Note |
|---|---|---|---|
| L-703 | SUP-003 | lead | seccomp/Landlock profile (security boundary) |
| H4 | gate | lead | cut bundle #4 (§9) once L-701..704 merge → unblocks B11, B12 |

## 3. Load and data generation (A8, L-801 to L-817)

Scale: L0 100k docs and L1-lite 1M docs, disk at most 12 GB.

| Item | Req | Who | Note |
|---|---|---|---|
| L-809 | LOAD-014 | runner | 300 indices campaign |
| L-810 | LOAD-015 | impl/runner → lead | large docs; 413/limit wording is lead's |
| L-811 | LOAD-017 | impl | done (L-811): leak check in teardown of each run (script change in scripts/load) |
| L-812 | LOAD-018 | runner | data streams 1M |
| L-813 | LOAD-020 | lead | trend store / regression gate design (CI) |
| L-814 | DATA-002 | impl | edgen mapping/settings per engine (single crate tools/edgen) |
| L-815 | DATA-003 | impl/runner → lead | edge catalogue `--profile edge` (tools/edgen/src/edge.rs exists; check first); expectations reviewed by lead |
| L-816 | DATA-007 | impl | OSB generate-data cross-check |
| L-817 | DATA-008 | impl | corpus generator with `--seed` |
| L-811a | LOAD-017 | impl | user decision 2026-11-01: end-of-run leak check for the Rust integration test runs (open scroll/PIT contexts, `.part` files incl. hidden `.name.part-*`, stray hidden indices) after the whole IT run, failing the run on a leak; reuse the L-811 checks; after L-817, before D-1 |
| Final | gate | lead | bundle after A8 → B2 (full), B10 |

## 4. Big-host campaigns (B0 to B13)

| Item | Req | Who | Depends | Note |
|---|---|---|---|---|
| B0 | — | runner | H0 ✓ | `cards/T-B0` |
| B1 | DATA-004/005/010 | runner → lead | H0/H1 ✓ | `cards/T-B1`; lead pins datasets.toml |
| B2 | LOAD-004…012, 014 | runner (cells) / lead (matrix design) | H1 ✓, A8 preferred | start with `cards/T-B2a`, then one card per scenario × engine |
| B3 | LOAD-005/008/009 at L2 | runner | B2 | big-host campaign; big5-100 / full http_logs need lead to add manifest entries |
| B4 | LOAD-013 | runner | B2, H3 ✓ | two clusters at once: only if memory allows |
| B5 | LOAD-016, DATA-009 | runner | B3 | 6–12 h soak; `SOAK_RUNNER` var (owner item) |
| B6 | LOAD-019, DATA-006 | impl/runner → lead | B1 | Rally/OSB baselines need owner Q8 (Rally/OSB) answered |
| B7 | API-084, INJ-015, SUP-020/022 | runner → lead | H2 ✓ | `cards/T-B7` |
| B8 | BRK-NET-009, BRK-SRV-012 | lead (design) → impl | H1 ✓ | multi-node compose and scenarios are new harness code |
| B9 | API-070, API-072 | runner | H3 ✓ | `scripts/it/xmatrix.sh all`, `xpairs.sh`, serialised |
| B10 | LOAD-010/011 | runner | B2 | dedicated cores, 3 runs each |
| B11 | SUP-010 | impl/runner → lead | H4 | second machine/VM needed |
| B12 | SUP-002/003 | lead | H4, B2 | root eBPF: optional, ask the owner |
| B13 | GAPS §3/§4 | O | L-A04/A05 + owner approval | **blocked**: no AWS without owner approval and a named sandbox account |
