# Testing in progress

The task plan delegated to the other agent (Grok 4.7 through OpenCode, using [`project/handoff/HANDOFF-GROK-OPENCODE.md`](project/handoff/HANDOFF-GROK-OPENCODE.md)). Everything below is assigned to it; it commits each task to `main` as `L-NNN: ...` and adds a row to [`project/plan/PROGRESS.md`](project/plan/PROGRESS.md). Finished tasks move to [TESTING-COMPLETED.md](TESTING-COMPLETED.md). Tests outside this plan are in [TESTING-TODO.md](TESTING-TODO.md).

Status as of 2026-11-15: work through L-808m (2026-11-09) is recorded in [TESTING-COMPLETED.md](TESTING-COMPLETED.md) and in the dated note in [`project/plan/PROGRESS.md`](project/plan/PROGRESS.md). Later work on this tree includes L-809 (OpenDAL storage), the searchdump-rs rename, 1.0.0-beta1/1.0.0, and the owner's 2026-11-14 decisions: L-645b (`--httpAuthFile` wins; done), L-810i (OpenSSH client in the images; CVEs accepted 2026-11-14, merged), release/1.1 1.1-4a (slice cap only on ES 7.10; done on release/1.1), L-804a (exit-code review; done), L-804b (proposals P1-P5 done: exit 4 for transient rerun-safe failures, `--restartOnContextLoss`, PIT and cluster-block hints; P6 deferred until a big host is available). L-808n to L-808r (restore CPU profile and fixes; CPU about half, level with M6; done). **Still open from this plan: L-703** (seccomp/Landlock). Owner decision item D-1 is still not started. Deferred for later owner review (2026-11-14): see the table in section 1. L-809 (LOAD-014 A: 300 indices, 5,000-field mapping, 100 aliases; done, PASS), L-810 (LOAD-015 large documents; done, PASS), L-812 (LOAD-018 data streams 1M; done, PASS), L-804c (CI `it` fix after L-804b: l618 expects exit 4; IT gate is now `scripts/it/run.sh <profile>`, the earlier "14 known failures" list was wrong). L-810j (owner decision 2026-11-14: the passwd/adduser/init-system-helpers tools that openssh-client pulls in are deleted from the Debian images; the executables check stays strict, no allowlist change; fixes the `security` image job failure since L-810i), L-810k (examples/ updated for exit 4, `--httpAuthFile` precedence, `--restartOnContextLoss`, the OpenSSH client in images built from main, the small-heap `searchdump-multi` overrides; 7 invalid Kubernetes manifests fixed). 1.1-10 is fixed on `release/1.1` only (whether to port it to `main` is the owner's call). L-818 (owner request: standard low-memory engine test set `scripts/load/lowheap.py`; PASS 16/16 on release/1.1, the old flags fail 23 of 32 runs; on main the default mode fails until 1.1-10 is ported). L-819 (release/1.1: the empty `FAILED  IX: :` context fixed, FEATURES-TODO 1.1-10 row, version 1.1.0; not tagged). L-820 (owner decision t103u: release/1.1 merged into main; main now carries 1.1.0 and lowheap.py passes 16/16 on main; release/1.1 fast-forwarded to the merge). L-821 (examples/ refreshed for 1.1.0: image `1.1.0-dhi-debian`, "New in 1.1.0" section). L-822 (`project/HANDOFF-1.1.md` refreshed; kvmsvr1 perf-run handoff in section 5b). The grokbox 16-engine 1.1.0 RC run is in progress (L-823). L-824 (owner request t108u: 1.2 plan `project/plan/ROADMAP-1.2-PLAN.md`, FEATURES-TODO 1.2-1 `--serverLimits`; docs only, 1.1.0 frozen). For the lead: `api::alias_round_trip_keeps_every_property` fails on elasticsearch-secure (alias on `.security-7`; not in CI). **Next:** L-814 (L-813 is lead-only), awaiting the owner's go-ahead.

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
| ES 9.1.2 reruns | done on `main` in bcc655c (2026-11-13), before the deferral | The isolated reruns of the three ES 9.1.2 failures (plain dump-edge/restore-edge, secure dump-zstd) all passed; the v1.0.0-beta1 L0 report has 576 PASS, 0 FAIL. Listing them as pending on 2026-11-14 was a tracking error. |
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

## 3. Load and data generation (A8, L-801 to L-818)

Scale: L0 100k docs and L1-lite 1M docs, disk at most 12 GB.

| Item | Req | Who | Note |
|---|---|---|---|
| L-809 | LOAD-014 | runner | done (L-809): 300 indices, 5,000-field mapping, 100 aliases PASS; 1,000 indices is Phase B (needs PIT or a raised `search.max_open_scroll_context`) |
| L-810 | LOAD-015 | impl/runner → lead | done (L-810): large docs PASS; 413 message now names the limit (wording for lead review); ~5.3x RSS per very large document noted |
| L-811 | LOAD-017 | impl | done (L-811): leak check in teardown of each run (script change in scripts/load) |
| L-812 | LOAD-018 | runner | done (L-812): 1M into a data stream with rollover, dump, host-only and rename PASS; routed documents into a data stream now warned once |
| L-813 | LOAD-020 | lead | trend store / regression gate design (CI) |
| L-814 | DATA-002 | impl | edgen mapping/settings per engine (single crate tools/edgen) |
| L-815 | DATA-003 | impl/runner → lead | edge catalogue `--profile edge` (tools/edgen/src/edge.rs exists; check first); expectations reviewed by lead |
| L-816 | DATA-007 | impl | OSB generate-data cross-check |
| L-817 | DATA-008 | impl | corpus generator with `--seed` |
| L-818 | LOAD-LM | impl/runner | done (L-818, owner request 2026-11-14): low-memory set `scripts/load/lowheap.py` (ES 7.10.2/8.6.1/8.19.23/9.1.2 at 256m and 384m, `searchdump-multi` dump and load of pmc-like and fat documents with default flags; `--regression` for the old flags). release/1.1 PASS 16/16; main fails by design until 1.1-10 is ported |
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
