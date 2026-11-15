#!/usr/bin/env bash
# Load a searchdump-multi dump directory from the mounted volume, including
# split parts. --skipExisting: set SKIP_EXISTING=1. 1.2: bulk sizes come from the target
# (--serverLimits=${SERVER_LIMITS:-auto}); manual override (warns above the target's limits):
# LIMIT, CONCURRENCY, MAX_BULK_BYTES, PARALLEL (the 1.1 small-heap recipe: LIMIT=500
# MAX_BULK_BYTES=16mb PARALLEL=1). SERVER_LIMITS=off restores the 1.1.0 sizes.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${SERVER_LIMITS:=auto}"
: "${PARALLEL:=}"

MULTI=(
  --direction=load
  --input=/data
  --output="${DEST_ES_URL}"
  --serverLimits="${SERVER_LIMITS}"
)
if [[ -n "${PARALLEL}" ]]; then
  MULTI+=(--parallel="${PARALLEL}")
fi
CHILD=()
for kv in "limit=${LIMIT:-}" "concurrency=${CONCURRENCY:-}" "maxBulkBytes=${MAX_BULK_BYTES:-}"; do
  if [[ -n "${kv#*=}" ]]; then
    CHILD+=(--"${kv}")
  fi
done
if [[ -n "${SKIP_EXISTING:-}" ]]; then
  MULTI+=(--skipExisting)
fi
if [[ -n "${PREFIX:-}" ]]; then
  MULTI+=(--prefix="${PREFIX}")
fi
if [[ -n "${SUFFIX:-}" ]]; then
  MULTI+=(--suffix="${SUFFIX}")
fi
run_searchdump_multi multi "${MULTI[@]}" ${CHILD[@]+-- "${CHILD[@]}"}
echo "loaded ${DUMP_DIR} -> ${DEST_ES_URL}"
