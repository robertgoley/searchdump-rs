#!/usr/bin/env bash
# Safe simple restore of one index inside the image (1.2): settings, mapping, then data, with NO
# sizing flags; the bulk size comes from the target's limits (--serverLimits, default auto).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

if [[ ! -f "${DUMP_DIR}/${INDEX}.data.json.gz" ]]; then
  echo "error: ${DUMP_DIR}/${INDEX}.data.json.gz not found (run safe-simple-dump.sh first)" >&2
  exit 1
fi
preflight run_searchdump output --input="/data/${INDEX}.data.json.gz" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" --type=data
for t in settings mapping data; do
  f="/data/${INDEX}.${t}.json"
  [[ "${t}" == data ]] && f="/data/${INDEX}.data.json.gz"
  run_searchdump output \
    --input="${f}" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type="${t}" \
    --serverLimits="${SERVER_LIMITS}"
done
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
