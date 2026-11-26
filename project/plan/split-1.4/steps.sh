#!/bin/bash
# steps.sh FROM TO FIRST_ID: for each split step, regenerate, gate, and commit (named files) when the gate is OK
# and the multi bin still runs 29 unit tests; stops at the first failure. Log: /tmp/steps.log
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
H=$(dirname "$0")
declare -A MSG=(
 [4]="sizing.rs (former pub mod sizing) and flags.rs (default/merged/cluster child flags), 6 tests move"
 [5]="selection.rs (Selector, targets, dir names) and types.rs (types, Node type flags), 5 tests move"
 [6]="files.rs (naming, parts, counts, meta dir), manifest.rs, discover.rs, 5 tests move"
 [7]="tls.rs (Secret, Side, ClusterTls, NODE_TLS_ENV; L4-001/L4-006) and cluster.rs (parent client, retries), 4 tests move"
 [8]="child/{mod,stats,probe,runner}.rs (child counts, binary probes, Runner with L4-002 ids), 7 tests + script_runner move"
 [9]="ctx.rs (Outcome, Ctx, Pacer) and planner.rs (plan_sizes, size notes), 1 test moves"
 [10]="dump.rs, load.rs, storage.rs, run_local/mod.rs; main.rs is docs, imports, mod decls and main"
)
id=$3
for n in $(seq "$1" "$2"); do
  echo "=== step $n" >> /tmp/steps.log
  "$H/apply.sh" "$n" >> /tmp/steps.log 2>&1 || { echo "apply failed" >> /tmp/steps.log; exit 1; }
  "$H/gate-fast.sh" > "/tmp/gate-s$n.log" 2>&1 < /dev/null; rc=$?
  grep -v UNHANDLED "/tmp/gate-s$n.log" | grep -E 'GATE|test result|ok=|identical' >> /tmp/steps.log
  if [ $rc -ne 0 ] || ! grep -q 'test result: ok. 29 passed' "/tmp/gate-s$n.log"; then echo "STOP at step $n (rc=$rc)" >> /tmp/steps.log; exit 1; fi
  files=$(git ls-files -mo --exclude-standard src/bin/searchdump-multi)
  git add $files
  git commit -q -m "L4-0$id: split step $n (L4-S$(printf %02d $n)): searchdump-multi ${MSG[$n]}; verbatim move (private items moved across modules become pub(crate))" >> /tmp/steps.log 2>&1
  git log --oneline -1 >> /tmp/steps.log
  id=$((id+1))
done
echo "ALL DONE" >> /tmp/steps.log
