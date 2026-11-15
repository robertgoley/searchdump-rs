#!/usr/bin/env bash
# Dump every index matching MATCH with searchdump-multi inside the image.
#
# Overrides the default --limit=5000 --maxResponseMb=0 (main and 1.0.0), which
# can 429 or OOM a small-heap Elasticsearch node even with --parallel=1.
# For a private CA, set SSL_CERT_FILE_HOST (mounted as SSL_CERT_FILE); the
# multi binary has no --ca on main.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${PARALLEL:=2}"
: "${LIMIT:=500}"
: "${COMPRESSION:=zstd1}"
: "${SCROLL_TIME:=10m}"

MULTI=(
  --direction=dump
  --input="${ES_URL}"
  --output=/data
  --match="${MATCH}"
  --parallel="${PARALLEL}"
  --compression="${COMPRESSION}"
)
if [[ -n "${RESUME:-}" ]]; then
  MULTI+=(--resume)
fi
run_searchdump_multi multi "${MULTI[@]}" -- \
  --limit="${LIMIT}" \
  --maxResponseMb="${MAX_RESPONSE_MB:-64}" \
  --scrollTime="${SCROLL_TIME}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
