# 1.2 progress note (wip/1.2-sizing; for whoever resumes after an interruption)

Updated: 2026-11-15 06:35 EST (box local time).

## Where things are
- `origin/release/1.2`: L2-001 (73d38f5, 1.2-1a), L2-002 (67d8ee6, 1.2-1b). Phase commits continue as L2-003..
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
- [ ] 1c L2-003: gates-1c OK (fmt/clippy/tests); first IT attempt failed on infra (the isolated store's
  layers were broken by the 05:55 box restore: overlay 'lower layer does not exist' / Permission denied);
  store reset with `podman system reset --force` (isolated store only) and images re-pulled
  (/workspace/logs12/pull.log); IT rerun next.
- gates on the full wip tree (gates-wip): fmt, clippy, lib+bins 309, all 10 test targets pass.
- [ ] 1d L2-004, 1e L2-005, 1f L2-006, 1h L2-007, 1g L2-008
