#!/usr/bin/env bash
# Safe simple restore of a searchdump-multi dump directory (1.2). NO page, bulk or worker flags:
# --serverLimits (default auto) sizes the bulks from the target's indexing pressure and
# http.max_content_length. SERVER_LIMITS=min for a fragile or shared cluster, off for 1.1.0.
#
#   DEST_ES_URL=https://dst:9200 DUMP_DIR=/data ./safe-simple-multi-restore.sh
#
# Credentials: SEARCHDUMP_OUTPUT_USERNAME / SEARCHDUMP_OUTPUT_PASSWORD, or HTTP_AUTH_FILE.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

need_bin "${SEARCHDUMP_MULTI_BIN}"
: "${SERVER_LIMITS:=auto}"

MULTI_FLAGS=(--direction=load --input="${DUMP_DIR}" --output="${DEST_ES_URL}")
if [[ -n "${HTTP_AUTH_FILE}" ]]; then
  MULTI_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
fi
if [[ -n "${MATCH_RESTORE:-}" ]]; then
  MULTI_FLAGS+=(--match="${MATCH_RESTORE}")
fi
preflight "${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}"
"${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}" --serverLimits="${SERVER_LIMITS}"
echo "loaded ${DUMP_DIR} -> ${DEST_ES_URL}"
