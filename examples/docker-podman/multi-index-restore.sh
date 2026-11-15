#!/usr/bin/env bash
# Load a searchdump-multi dump directory from the mounted volume, including
# split parts. --skipExisting: set SKIP_EXISTING=1. Bulks are capped at
# MAX_BULK_BYTES (default 16mb) so a small-heap target does not answer 429.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${PARALLEL:=2}"
: "${LIMIT:=500}"

MULTI=(
  --direction=load
  --input=/data
  --output="${DEST_ES_URL}"
  --parallel="${PARALLEL}"
)
if [[ -n "${SKIP_EXISTING:-}" ]]; then
  MULTI+=(--skipExisting)
fi
if [[ -n "${PREFIX:-}" ]]; then
  MULTI+=(--prefix="${PREFIX}")
fi
if [[ -n "${SUFFIX:-}" ]]; then
  MULTI+=(--suffix="${SUFFIX}")
fi
run_searchdump_multi multi "${MULTI[@]}" -- \
  --limit="${LIMIT}" \
  --concurrency="${CONCURRENCY:-2}" \
  --maxBulkBytes="${MAX_BULK_BYTES:-16mb}" \
  --maxResponseMb="${MAX_RESPONSE_MB:-64}"
echo "loaded ${DUMP_DIR} -> ${DEST_ES_URL}"
