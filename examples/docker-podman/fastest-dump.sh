#!/usr/bin/env bash
# Fast dump: large pages, more workers, zstd level 1, optional sliced PIT.
# Set SLICES to the primary shard count (0 = unsliced). 1.1.0 caps it on
# ES 7.10 only (--sliceCap=auto); too many slices can be slower there.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=2000}"
: "${WORKERS:=8}"
: "${SLICES:=0}"

rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.settings.json" \
  --type=settings --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.mapping.json" \
  --type=mapping --overwrite
FAST=(
  --input="${ES_URL}/${INDEX}"
  --output="/data/${INDEX}.data.json.zst"
  --type=data --overwrite
  --compressionType=zstd --compressionLevel=1 --esCompress
  --limit="${LIMIT}" --workers="${WORKERS}" --bufferSize=32 --maxResponseMb=256
)
if [[ "${SLICES}" != "0" ]]; then
  FAST+=(--searchAfter --pit --slices="${SLICES}")
fi
rerun_on_4 run_searchdump input "${FAST[@]}"
echo "wrote ${DUMP_DIR}/${INDEX}.data.json.zst (SLICES=${SLICES})"
