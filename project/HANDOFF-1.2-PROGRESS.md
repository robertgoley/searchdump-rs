# 1.2 progress note (wip/1.2-sizing; for whoever resumes after an interruption)

Updated: 2026-11-15 08:08 EST (box local time).

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
- [ ] 1f L2-006: `/workspace/perf-1.2/chain-1f.sh` (nohup; waits for chain-1e CHAIN-DONE; resumable) runs
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
