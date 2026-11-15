#!/usr/bin/env bash
# Dump every index matching MATCH (default ^logs-) with searchdump-multi.
#
# IMPORTANT: in 1.0.0, data children default to --limit=5000 and
# --maxResponseMb=0 (1.1.0: 1000 per page, response cap kept). That can 429 or OOM a small-heap Elasticsearch node even
# with --parallel=1 (reproduced on 256 MB-heap ES 7.10.2 / 8.6.1 / 8.19.23).
# This script always overrides the page size (-- --limit=500
# --maxResponseMb=64). Raise LIMIT / PARALLEL on a large, well-resourced
# cluster. Add RESUME=1 (--resume) to finish a previous run; a failed index
# (searchdump-multi exit 1) is safe to retry that way.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

need_bin "${SEARCHDUMP_MULTI_BIN}"

: "${PARALLEL:=2}"
: "${LIMIT:=500}"
: "${COMPRESSION:=zstd1}"
: "${SCROLL_TIME:=10m}"

ensure_dump_dir
MULTI_FLAGS=(
  --direction=dump
  --input="${ES_URL}"
  --output="${DUMP_DIR}"
  --match="${MATCH}"
  --parallel="${PARALLEL}"
  --compression="${COMPRESSION}"
)
if [[ -n "${HTTP_AUTH_FILE}" ]]; then
  MULTI_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
fi
if [[ -n "${RESUME:-}" ]]; then
  MULTI_FLAGS+=(--resume)
fi
# Private CA: searchdump-multi has no --ca; honour SSL_CERT_FILE instead
# (already in the environment if you set it).
"${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}" -- \
  --limit="${LIMIT}" \
  --maxResponseMb="${MAX_RESPONSE_MB:-64}" \
  --scrollTime="${SCROLL_TIME}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
