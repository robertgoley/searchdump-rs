#!/usr/bin/env bash
# Dump one index with a large page size, more workers, zstd level 1, and sliced
# PIT when the engine supports it.
#
# Set SLICES to the primary shard count of the index. Values higher than the
# shard count can be slower on Elasticsearch 7.10 and earlier (and some
# OpenSearch builds). 1.1.0 caps it on ES 7.10 only (--sliceCap=auto); elsewhere
# you pick N yourself.
# If the cluster has no PIT, drop --searchAfter and --pit and the run falls
# back to scroll (or omit them up front).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=2000}"
: "${WORKERS:=8}"
: "${SLICES:=0}"

ensure_dump_dir
dump_settings_and_mapping
input_cluster_flags
DUMP_ARGS=(
  --input="${ES_URL}/${INDEX}"
  --output="$(data_file json.zst)"
  --type=data
  --overwrite
  --compressionType=zstd
  --compressionLevel=1
  --esCompress
  --limit="${LIMIT}"
  --workers="${WORKERS}"
  --bufferSize=32
  --maxResponseMb=256
)
if [[ "${SLICES}" != "0" ]]; then
  DUMP_ARGS+=(--searchAfter --pit --slices="${SLICES}")
fi
rerun_on_4 "${SEARCHDUMP_BIN}" "${DUMP_ARGS[@]}" "${INPUT_CLUSTER_FLAGS[@]}"
echo "wrote $(data_file json.zst) (SLICES=${SLICES})"
