#!/usr/bin/env bash
# Copy one index from ES_URL to DEST_ES_URL (settings, mapping, then data).
# This is searchdump cluster-to-cluster, not searchdump-multi --direction=copy.
# Set SEARCHDUMP_INPUT_* and SEARCHDUMP_OUTPUT_* when the users differ, and
# leave HTTP_AUTH_FILE unset then: since 1.1.0 the auth file wins.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=1000}"
: "${CONCURRENCY:=4}"

if [[ "${ES_URL}" == "${DEST_ES_URL}" && "${INDEX}" == "${DEST_INDEX}" ]]; then
  echo "error: source and destination are the same; set DEST_ES_URL or DEST_INDEX" >&2
  exit 1
fi

warn_auth_file_wins
for t in settings mapping data; do
  EXTRA=()
  if [[ "${t}" == "data" ]]; then
    EXTRA+=(--limit="${LIMIT}" --concurrency="${CONCURRENCY}" --workers=4 --noRefresh --esCompress)
  fi
  rerun_on_4 run_searchdump both \
    --input="${ES_URL}/${INDEX}" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type="${t}" \
    "${EXTRA[@]}"
done
echo "copied ${ES_URL}/${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
