#!/usr/bin/env bash
# Safe simple restore of one index (1.2): settings, mapping, then data, with NO sizing flags.
# The bulk size comes from the target's limits (--serverLimits, default auto; min for a fragile
# or shared cluster; off for the 1.1.0 sizes). A preflight with --serverLimits=check stops the
# script on exit 2.
#
#   DEST_ES_URL=https://dst:9200 INDEX=logs DUMP_DIR=/data ./safe-simple-restore.sh
#
# Credentials: SEARCHDUMP_OUTPUT_USERNAME / SEARCHDUMP_OUTPUT_PASSWORD, or HTTP_AUTH_FILE.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

data="${DUMP_DIR}/${INDEX}.data.json.gz"
if [[ ! -f "${data}" ]]; then
  echo "error: ${data} not found (run safe-simple-dump.sh first)" >&2
  exit 1
fi
output_cluster_flags
preflight "${SEARCHDUMP_BIN}" --input="${data}" --output="${DEST_ES_URL}/${DEST_INDEX}" --type=data \
  "${OUTPUT_CLUSTER_FLAGS[@]}"
for t in settings mapping data; do
  f="${DUMP_DIR}/${INDEX}.${t}.json"
  [[ "${t}" == data ]] && f="${data}"
  "${SEARCHDUMP_BIN}" \
    --input="${f}" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type="${t}" \
    --serverLimits="${SERVER_LIMITS}" \
    "${OUTPUT_CLUSTER_FLAGS[@]}"
done
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
