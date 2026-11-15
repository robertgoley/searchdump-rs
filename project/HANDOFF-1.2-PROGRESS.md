# 1.2 progress note (wip/1.2-sizing; for whoever resumes after an interruption)

Updated: 2026-11-15 07:25 EST (box local time).

## Where things are
- `origin/release/1.2`: L2-001 (73d38f5, 1.2-1a), L2-002 (67d8ee6, 1.2-1b), L2-003 (c07e00f, 1.2-1c),
  L2-004 (6acedfa, 1.2-1d). Phase commits continue as L2-005..
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
- [ ] 1e L2-005: chk tree has wip searchdump-multi.rs + cli_contract.rs + results/local/L2-005.md (draft from
  /workspace/logs12/drafts); gates-1e running (/workspace/perf-1.2/gates-1e, nohup log
  /workspace/logs12/gates-1e.out); then IT-1e and a live multi check (perf-1.2/live/multi_live.py; it read
  `manifest.json`, the manifest is `multielasticdump.json`: script fix needed). /var/tmp/searchdump-load
  was wiped by the reboot, lowheap data regenerates.
- [ ] 1f L2-006, 1h L2-007, 1g L2-008
