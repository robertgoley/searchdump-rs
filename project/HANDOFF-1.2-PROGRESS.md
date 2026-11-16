# 1.2 progress note (wip/1.2-sizing; for whoever resumes after an interruption)

Updated: 2026-11-15 19:40 EST (box local time).

## Where things are
- `origin/release/1.2`: L2-001 (73d38f5, 1.2-1a), L2-002 (67d8ee6, 1.2-1b), L2-003 (c07e00f, 1.2-1c),
  L2-004 (6acedfa, 1.2-1d), L2-005 (9114b8c, 1.2-1e), L2-007 (bcacb97, 1.2-1h; pushed before L2-006
  because 1f's lowheap run was still going). Remaining: L2-006 (1f), L2-008 (1g).
- `origin/wip/1.2-sizing`: all in-progress 1c-1h code (this branch). 16c28b4 preserved the interrupted
  worker's uncommitted tree (limits probe, 429 handling, multi sizing, lowheap modes, fixtures, safe-simple
  examples).
- Worktrees on the box: `/workspace/rel-1.2` (branch `rel12`, = wip/1.2-sizing, full 1.2 code);
  `/workspace/rel-1.2-chk` (branch `rel12r`, tracks origin/release/1.2; used to build each phase commit:
  phase files are copied from the wip tree, gated, committed, pushed).
- 1c (L2-003) files = src/http/retry.rs, src/pipeline/bulk.rs, src/retrieval/{pit_search,retrieval_task,
  scroll,search_after}.rs, tests/chaos_brk.rs (identical to the wip versions).

## Scripts and logs (box)
- env: `source /workspace/perf-1.2/env.sh` (isolated podman store ~/.local/share/edrs-1.2-podman,
  XDG_RUNTIME_DIR=/tmp/xdg-runtime-1.2, perf port 19252, IT port 19260, Rust from ~/.cargo/bin = 1.91.1).
- gates: `/workspace/perf-1.2/gates.sh TREE TAG` -> `/workspace/perf-1.2/gates-TAG/summary.txt`.
- IT: `/workspace/perf-1.2/it.sh TREE TAG` (es710-plain es819-plain elasticsearch-plain opensearch-plain
  es710-secure) -> `/workspace/perf-1.2/it-TAG/summary.txt`.
- tracking docs: `TREE=<worktree> python3 /workspace/perf-1.2/track.py L2-NNN 1.2-1x "result" "next"`.
- lowheap/IT share `/tmp/searchdump-it.lock`, so they serialize.

## 1.1.0 RC run
Nothing of it was running when this worker started (box rebooted ~06:00 EST; no RC processes,
containers or /workspace/perf-pit-1.1). Nothing else removed.

## Done / next
- [x] preserve uncommitted tree (16c28b4)
- [x] 1c L2-003 c07e00f: gates-1c OK; IT 5 profiles 0 failed (/workspace/perf-1.2/it-1c, after the isolated
  store reset + re-pull).
- [x] 1d L2-004 6acedfa: built in /workspace/rel-1.2-p (detached 1d tree, = chk tree at L2-004);
  gates-1d OK (lib+bins 333, cli_contract 70); IT 5 profiles 0 failed (/workspace/perf-1.2/it-1d).
- [x] 1e L2-005 9114b8c: gates OK except the known ETXTBSY test-helper flake (5/5 reruns pass); IT-1e 5 profiles
  0 failed; live multi 256m es819/es710 all modes pass (live/multi-*.json).
- perf binaries frozen from the L2-005 tree: /workspace/perf-1.2/bin-1.2rc (searchdump, searchdump-multi, edgen,
  edverify). `/workspace/perf-1.2/chain-1g.sh` (nohup; waits for chain-1f CHAIN-DONE): safe-simple live
  (es710, es819, os2 256m -> live/safe-*.json, log logs12/chain-1g.log), then the 16-engine L0 suite with
  bin-1.2rc (--skip-node, reps 3; resumable per engine; summary /workspace/logs12/perf-1.2.summary), then
  1.1.0 back-to-back on es710-secure and es819-plain. Relaunch with `nohup ./chain-1g.sh &` after a reboot.
- [ ] 1f L2-006 FINISH RECIPE (after `CHAIN-DONE` in logs12/chain-1f.log): `/workspace/perf-1.2/make_l2006.sh`
  (copies results/load/L2-006-*/lowheap.json -> results/local/L2-006-<mode>-lowheap.json, writes
  results/local/L2-006.md via perf-1.2/lowheap_agg.py); review the Notes (os3/384m auto load-fat 104.9 s outlier);
  examples/README.md "Minimum resources" 1.2 bullet ALREADY EDITED in the chk tree (uncommitted; adjust if
  off/min differ); 1g compare script ready: /workspace/perf-1.2/perf_compare.py (reads logs12/perf-1.2.summary); `TREE=/workspace/rel-1.2-chk python3 /workspace/perf-1.2/track.py L2-006 1.2-1f "..." "..."`;
  `git add` scripts/load/{lowheap.py,README.md} results/local/L2-006* examples/README.md + the 8 tracking docs
  (never -A); secret-scan (source perf-1.2/env.sh first: needs rustc 1.91 from ~/.cargo/bin); push HEAD:release/1.2.
  auto DONE 08:12: 14/14 cells PASS (ES 7.10/8.6/8.19/9.1, OS 1.3.20/2.19.7/3.9.0 at 256m+384m); 1.0 flags 20/56 fail.
  off started 08:12: 12/14 cells done at 08:40, 0 failures (~2.5 min/cell; box shared with another agent's node
  process). ETA: off ~08:45, v110 ~09:20, min ~09:55 = chain-1f CHAIN-DONE; chain-1g then safe-simple live
  (~10 min), 16-engine L0 perf (~10 h, ETA ~20:00-21:00 EST), 1.1.0 back-to-back (~1-1.5 h) -> PERF-DONE ~22:00 EST.
  1g: `python3 /workspace/perf-1.2/perf_compare.py` (self-tested on a beta1 dir) for ratios vs the 16 beta1 dirs
  named in scripts/perf/results/v1.0.0-beta1-L0.md; L2-007 live safe-simple results go into L2-008.
- [ ] 1f L2-006 (orig): `/workspace/perf-1.2/chain-1f.sh` (nohup; waits for chain-1e CHAIN-DONE; resumable) runs
  lowheap on 7 engines x 256m/384m: L2-006-auto (+--regression), -off, -v110 (1.1.0 bins, --mode none), -min;
  logs /workspace/logs12/lowheap-*.log, chain-1f.log; JSON in rel-1.2-chk/results/load/L2-006-*/.
  lowheap.py + scripts/load/README.md already copied into the chk tree (uncommitted, part of L2-006).
- [x] 1h L2-007 bcacb97 (results/local/L2-007.md; live safe-simple cells pending -> record in L2-008):
  shellcheck (0.11) + kubeconform -strict on the wip examples: clean / 18 valid.
  Live harness /workspace/perf-1.2/live/safe_simple.py ENGINE HEAP PORT (bash safe-simple scripts) to run
  after chain-1f on es710, es819, os2 at 256m.
- [ ] 1g L2-008: perf plan: 1.1.0 RC raw data is not on the box (L-823 never landed); compare the 1.2 16-engine
  L0 (--skip-node, reps 3) to the beta1 raw dirs in /workspace/fix-wt/results/perf/20261111T* (1.1.0 = beta1 for
  the suite's multi cases, L2-001) plus a 1.1.0-vs-1.2 back-to-back on es710-secure and es819-plain.

## L2-009 candidate (wip only; NOT on release/1.2; goes into L2-008 or a new L2-NNN after review)
- chain-1g safe-simple live (14:09-14:10 EST, es710/es819/os2 256m) all FAILED at single-dump-pmc:
  exit 2, "preflight: explicit flags exceed what the cluster can take", probed=false, then the restore
  step found no dump file.
- Root cause: NOT the auto/min explicit-value policy (auto keeps an explicit value with a WARNING; only
  check exits 2, as ROADMAP-1.2 section 2 says; contract test l2_004 already covers it). The safe-simple
  preflight runs `searchdump --input=URL/INDEX --output=/dev/null --type=data --serverLimits=check`, and
  validate() (L-644, TST-BRK-CLI-005) refused /dev/null as "a character device" with the usage exit 2
  before the probe ran (hence no "server limits" line, probed=false). `_preflight.sh` maps every exit 2
  to "explicit flags exceed". Affected: bash + docker-podman safe-simple-dump.sh and
  kubernetes/safe-simple-dump.yaml (restore and multi preflights name a cluster/dir and were fine).
- Fix (wip): src/lib.rs validate() skips check_output_path when --serverLimits=check (check never opens
  the output); _preflight.sh (bash, docker-podman) and examples/README.md say exit 2 may also be a usage
  error. Tests: cli_contract l2_009_check_preflight_accepts_dev_null_output (check + /dev/null -> 0;
  check + --limit 5000 -> 2; auto + /dev/null still refused) and l2_009_safe_simple_dump_example_runs_end_to_end
  (runs examples/bash/safe-simple-dump.sh against a router mock; both fail without the lib.rs change with
  the exact live message).
- Harness /workspace/perf-1.2/live/safe_simple.py (box only): keeps step logs in live/logs-ENGINE-HEAP/
  (the work dir rm lost the real error) and skips digest/restore after a failed dump. Backup safe_simple.py.bak-*.
- Rerun after perf (needs a binary with the fix; bin-1.2rc does not have it):
  build release in the wip tree, then `TREE=/workspace/rel-1.2 BIN=/workspace/rel-1.2/target/release
  python3 /workspace/perf-1.2/live/safe_simple.py es710 256m 19258` (and es819, os2).
- es819-plain 3h11m (15:48-19:00): not a 1.2 regression. 148 measured runs total 244 s wall, all exit 0,
  37 PASS, no 429/retry warnings. Two wall-clock gaps with no tool output: 16:04:24-17:57:36 (inside the
  restore-split-none warmup, whose own wall_s is 15.9 s) and 18:00:56-18:58:16 (between two harness verify
  files). wall_s comes from a monotonic timer, so the hours were the VM being paused (dmesg: 20 "crng reseeded
  due to virtual machine fork"; monotonic uptime 4h18m vs ~13h of wall time since the 05:55 restore).
  elasticsearch8-secure (50 min) likely the same. Timings stay valid; durations from summary timestamps do not.
