#!/usr/bin/env bash
# Load a searchdump-multi dump directory, including split parts, in order.
# --skipExisting skips an index that already exists on the target.
# 1.2: bulk sizes come from the target (--serverLimits=${SERVER_LIMITS:-auto}: indexing pressure
# and http.max_content_length), so no flags are needed on a small or a big cluster. Manual
# override (warns when above the target's limits): LIMIT, CONCURRENCY, MAX_BULK_BYTES, PARALLEL,
# e.g. the 1.1 small-heap recipe LIMIT=500 MAX_BULK_BYTES=16mb PARALLEL=1. SERVER_LIMITS=off
# restores the 1.1.0 sizes.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

need_bin "${SEARCHDUMP_MULTI_BIN}"

: "${SERVER_LIMITS:=auto}"
: "${PARALLEL:=}"

MULTI_FLAGS=(
  --direction=load
  --input="${DUMP_DIR}"
  --output="${DEST_ES_URL}"
  --serverLimits="${SERVER_LIMITS}"
)
if [[ -n "${PARALLEL}" ]]; then
  MULTI_FLAGS+=(--parallel="${PARALLEL}")
fi
CHILD_FLAGS=()
for kv in "limit=${LIMIT:-}" "concurrency=${CONCURRENCY:-}" "maxBulkBytes=${MAX_BULK_BYTES:-}"; do
  if [[ -n "${kv#*=}" ]]; then
    CHILD_FLAGS+=(--"${kv}")
  fi
done
if [[ -n "${HTTP_AUTH_FILE}" ]]; then
  MULTI_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
fi
if [[ -n "${SKIP_EXISTING:-}" ]]; then
  MULTI_FLAGS+=(--skipExisting)
fi
if [[ -n "${PREFIX:-}" ]]; then
  MULTI_FLAGS+=(--prefix="${PREFIX}")
fi
if [[ -n "${SUFFIX:-}" ]]; then
  MULTI_FLAGS+=(--suffix="${SUFFIX}")
fi
"${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}" ${CHILD_FLAGS[@]+-- "${CHILD_FLAGS[@]}"}
echo "loaded ${DUMP_DIR} -> ${DEST_ES_URL}"
