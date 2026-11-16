#!/usr/bin/env bash
# Safe simple dump of every index matching MATCH with searchdump-multi (1.2). NO page, bulk or
# worker flags: --serverLimits (default auto) probes the cluster once, reads every index's
# document size in one call and chooses the documents per page and how many indices run at once
# (fewer indices before smaller pages). SERVER_LIMITS=min for a fragile or shared cluster,
# off for the 1.1.0 sizes. A preflight with --serverLimits=check prints the per-index plan.
#
#   ES_URL=https://src:9200 MATCH='^logs-' DUMP_DIR=/data ./safe-simple-multi-dump.sh
#
# Credentials: SEARCHDUMP_INPUT_USERNAME / SEARCHDUMP_INPUT_PASSWORD, or HTTP_AUTH_FILE.
# Add RESUME=1 to finish an interrupted run.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

need_bin "${SEARCHDUMP_MULTI_BIN}"
: "${SERVER_LIMITS:=auto}"

MULTI_FLAGS=(--direction=dump --input="${ES_URL}" --output="${DUMP_DIR}" --match="${MATCH}")
if [[ -n "${HTTP_AUTH_FILE}" ]]; then
  MULTI_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
fi
preflight "${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}"
if [[ -n "${RESUME:-}" ]]; then
  MULTI_FLAGS+=(--resume)
fi
ensure_dump_dir
"${SEARCHDUMP_MULTI_BIN}" "${MULTI_FLAGS[@]}" --serverLimits="${SERVER_LIMITS}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
