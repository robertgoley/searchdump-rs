#!/bin/bash
# G-full for the multi split (final verification). Logs: /tmp/gfull/*.log; summary on stdout.
[ -f /workspace/perf-1.4/env.sh ] && source /workspace/perf-1.4/env.sh
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
mkdir -p /tmp/gfull
run() { n=$1; shift; if "$@" > /tmp/gfull/$n.log 2>&1; then echo "PASS $n"; else echo "FAIL $n (rc=$?)"; fi; }
run release-build cargo build --release --bins
run clippy cargo clippy --workspace --lib --bins --tests -- -D warnings
run clippy-no-default cargo clippy --bin searchdump-multi --no-default-features -- -D warnings
run cargo-test cargo test --workspace
run nextest-unit cargo nextest run --profile unit -E "$(scripts/sup/flake-policy.py --filter)"
run nextest-contract cargo nextest run --profile contract -E "$(scripts/sup/flake-policy.py --filter)"
run help-diff bash -c 'COLUMNS=100 target/release/searchdump-multi --help | diff tests/fixtures/multi/help.txt - && COLUMNS=100 target/release/searchdump-multi -h | diff tests/fixtures/multi/help-short.txt -'
run geiger scripts/sup/geiger-gate.sh
run vet cargo vet
run deny cargo deny check
run audit cargo audit
run machete cargo machete
run pins scripts/it/check-pins.sh
run matrix-md scripts/it/matrix-md.py --check
run native-code scripts/sup/native-code-check.py
run notices scripts/third-party-notices.py --check
echo GFULL-DONE
