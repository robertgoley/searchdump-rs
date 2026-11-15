#!/usr/bin/env bash
# Dump every index matching MATCH (default ^logs-) with searchdump-multi.
#
# 1.2: sizing comes from the cluster (--serverLimits=${SERVER_LIMITS:-auto}): pages, and how many
# indices run at once, are chosen from the node's heap and breakers, so a 256 MB node and a big
# cluster both work without flags. Manual override (only when you know the cluster; a value above
# its limits warns): set LIMIT (and MAX_RESPONSE_MB) and/or PARALLEL, e.g. the 1.1 small-heap
# recipe LIMIT=500 MAX_RESPONSE_MB=64 PARALLEL=1. SERVER_LIMITS=off restores the 1.1.0 sizes.
# Add RESUME=1 (--resume) to finish a previous run; a failed index
# (searchdump-multi exit 1) is safe to retry that way.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

need_bin "${SEARCHDUMP_MULTI_BIN}"

: "${SERVER_LIMITS:=auto}"
: "${PARALLEL:=}"
: "${LIMIT:=}"
: "${COMPRESSION:=zstd1}"
: "${SCROLL_TIME:=10m}"

ensure_dump_dir
MULTI_FLAGS=(
  --direction=dump
  --input="${ES_URL}"
  --output="${DUMP_DIR}"
  --match="${MATCH}"
  --compression="${COMPRESSION}"
  --serverLimits="${SERVER_LIMITS}"
)
if [[ -n "${PARALLEL}" ]]; then
  MULTI_FLAGS+=(--parallel="${PARALLEL}")
fi
CHILD_FLAGS=(--scrollTime="${SCROLL_TIME}")
if [[ -n "${LIMIT}" ]]; then
  CHILD_FLAGS+=(--limit="${LIMIT}")
fi
if [[ -n "${MAX_RESPONSE_MB:-}" ]]; then
  CHILD_FLAGS+=(--maxResponseMb="${MAX_RESPONSE_MB}")
fi
if [[ -n "${HTTP_AUTH_FILE}" ]]; then
  MULTI_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
fi
if [[ -n "${RESUME:-}" ]]; then
  MULTI_FLAGS+=(--resume)
fi
# Private CA: searchdump-multi has no --ca; honour SSL_CERT_FILE instead
# (already in the environment if you set it).
"${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}" -- "${CHILD_FLAGS[@]}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
