#!/usr/bin/env bash
# Dump one index with small pages and a single worker, so a constrained cluster
# (including a small-heap bitnamilegacy Elasticsearch node) stays under its
# memory guard.
#
# 1.2: --serverLimits=min reads the node's heap and breakers and keeps every page within half of
# what auto would use (a fragile or shared cluster). searchdump defaults to --limit=100 and
# --concurrency=1; this script also pins --workers=1 and a 64 MB response cap (the 1.1 manual
# recipe, kept because it is already small; min only lowers the page further when needed).
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
  --serverLimits=min \
  "${INPUT_CLUSTER_FLAGS[@]}"
echo "wrote $(settings_file), $(mapping_file) and $(data_file json.gz)"
