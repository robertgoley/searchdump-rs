#!/usr/bin/env bash
# Dump one index with small pages and a single worker, so a constrained cluster
# (including a small-heap bitnamilegacy Elasticsearch node) stays under its
# memory guard.
#
# searchdump itself defaults to --limit=100 and --concurrency=1. This script
# also pins --workers=1 and a 64 MB response cap.
#
# If you switch this to searchdump-multi, do not rely on its defaults: on
# main and 1.0.0 every data child gets --limit=5000 and --maxResponseMb=0, which
# can 429 or OOM a small heap even with one thread and one shard. Pass
# -- --limit=500 --maxResponseMb=64 or --noDefaultDataFlags. See
# examples/README.md and FEATURES-TODO.md item 1.1-10.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

ensure_dump_dir
EXTRA_FLAGS=(--limit=100 --workers=1 --concurrency=1 --maxResponseMb=64)
dump_settings_and_mapping
input_cluster_flags
rerun_on_4 "${SEARCHDUMP_BIN}" \
  --input="${ES_URL}/${INDEX}" \
  --output="$(data_file json.gz)" \
  --type=data \
  --overwrite \
  --fsCompress \
  --limit=100 \
  --workers=1 \
  --concurrency=1 \
  --maxResponseMb=64 \
  "${INPUT_CLUSTER_FLAGS[@]}"
echo "wrote $(settings_file), $(mapping_file) and $(data_file json.gz)"
