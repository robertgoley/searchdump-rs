#!/usr/bin/env bash
# Dump every index matching MATCH with searchdump-multi inside the image.
#
# 1.2: pages and workers come from the cluster (--serverLimits=${SERVER_LIMITS:-auto}). Manual
# override (warns above the cluster's limits): LIMIT, MAX_RESPONSE_MB, PARALLEL (the 1.1
# small-heap recipe: LIMIT=500 MAX_RESPONSE_MB=64 PARALLEL=1). SERVER_LIMITS=off: 1.1.0 sizes.
# For a private CA, set SSL_CERT_FILE_HOST (mounted as SSL_CERT_FILE); the
# multi binary has no --ca.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${SERVER_LIMITS:=auto}"
: "${PARALLEL:=}"
: "${LIMIT:=}"
: "${COMPRESSION:=zstd1}"
: "${SCROLL_TIME:=10m}"

MULTI=(
  --direction=dump
  --input="${ES_URL}"
  --output=/data
  --match="${MATCH}"
  --compression="${COMPRESSION}"
  --serverLimits="${SERVER_LIMITS}"
)
if [[ -n "${PARALLEL}" ]]; then
  MULTI+=(--parallel="${PARALLEL}")
fi
CHILD=(--scrollTime="${SCROLL_TIME}")
if [[ -n "${LIMIT}" ]]; then
  CHILD+=(--limit="${LIMIT}")
fi
if [[ -n "${MAX_RESPONSE_MB:-}" ]]; then
  CHILD+=(--maxResponseMb="${MAX_RESPONSE_MB}")
fi
if [[ -n "${RESUME:-}" ]]; then
  MULTI+=(--resume)
fi
run_searchdump_multi multi "${MULTI[@]}" -- "${CHILD[@]}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
