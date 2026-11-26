#!/bin/bash
# G-fast gate for the multi split; run from anywhere in the worktree. Logs in /tmp/gate/
set -e
[ -f /workspace/perf-1.4/env.sh ] && source /workspace/perf-1.4/env.sh
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
mkdir -p /tmp/gate
step() { echo "== $1"; shift; if "$@" > /tmp/gate/cur.log 2>&1; then tail -2 /tmp/gate/cur.log; else cat /tmp/gate/cur.log | tail -40; echo "GATE FAIL"; exit 1; fi; }
[ "$1" = "--from-contract" ] || {
step fmt cargo fmt --check
step clippy cargo clippy -q --workspace --lib --bins --tests -- -D warnings
step check-no-default cargo check -q --bin searchdump-multi --no-default-features
step check-opendal cargo check -q --bin searchdump-multi --features opendal
step build cargo build -q --bins
step unit cargo nextest run --profile unit -E "$(scripts/sup/flake-policy.py --filter)"
step multi-bin-tests cargo test -q --bin searchdump-multi
}
step contract cargo nextest run --profile contract -E "binary(cli_contract)"
echo "== roundtrip (baseline: 47 ok, only the pre-existing 'fake node saw requests it does not handle' FAIL)"
scripts/it/multielasticdump-roundtrip.sh target/debug > /tmp/gate/rt.log 2>&1 || true
nok=$(grep -c '^ok' /tmp/gate/rt.log || true); fails=$(grep '^FAIL' /tmp/gate/rt.log || true)
echo "ok=$nok fails=[$fails]"
[ "$nok" = 47 ] && [ "$fails" = "FAIL  fake node saw requests it does not handle" ] || { echo "GATE FAIL (roundtrip)"; exit 1; }
echo "== help diff"
COLUMNS=100 target/debug/searchdump-multi --help | diff tests/fixtures/multi/help.txt -
COLUMNS=100 target/debug/searchdump-multi -h | diff tests/fixtures/multi/help-short.txt -
echo identical
echo "GATE OK"
