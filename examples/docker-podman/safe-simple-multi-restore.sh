#!/usr/bin/env bash
# Safe simple searchdump-multi restore inside the image (1.2): NO page, bulk or worker flags;
# --serverLimits (default auto) sizes the bulks from the target's limits.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

MULTI=(--direction=load --input=/data --output="${DEST_ES_URL}")
preflight run_searchdump_multi multi "${MULTI[@]}"
run_searchdump_multi multi "${MULTI[@]}" --serverLimits="${SERVER_LIMITS}"
echo "loaded ${DUMP_DIR} -> ${DEST_ES_URL}"
