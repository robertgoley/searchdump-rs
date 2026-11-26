#!/bin/bash
# apply.sh N: regenerate src/bin/searchdump-multi/ for split steps <= N, then cargo fmt
set -e
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
[ -f /workspace/perf-1.4/env.sh ] && source /workspace/perf-1.4/env.sh
D=src/bin/searchdump-multi
rm -rf "$D"/*
python3 $(dirname "$0")/split.py "$1" "$D"
cargo fmt
