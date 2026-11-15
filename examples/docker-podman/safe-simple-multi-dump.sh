#!/usr/bin/env bash
# Safe simple searchdump-multi dump inside the image (1.2): NO page, bulk or worker flags;
# --serverLimits (default auto) chooses pages and how many indices run at once. For a private CA
# set SSL_CERT_FILE_HOST (the multi binary has no --ca).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

MULTI=(--direction=dump --input="${ES_URL}" --output=/data --match="${MATCH}")
preflight run_searchdump_multi multi "${MULTI[@]}"
if [[ -n "${RESUME:-}" ]]; then
  MULTI+=(--resume)
fi
run_searchdump_multi multi "${MULTI[@]}" --serverLimits="${SERVER_LIMITS}"
echo "dumped indices matching ${MATCH} to ${DUMP_DIR}"
