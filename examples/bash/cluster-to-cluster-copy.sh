#!/usr/bin/env bash
# Copy one index from ES_URL to DEST_ES_URL (settings, mapping, then data).
# This is searchdump cluster-to-cluster, not searchdump-multi --direction=copy
# (that copies a dump directory between stores, with no cluster involved).
#
# Set SEARCHDUMP_INPUT_* and SEARCHDUMP_OUTPUT_* when the clusters use
# different users. HTTP_AUTH_FILE is used for both sides when the user is the
# same; since 1.1.0 it wins over SEARCHDUMP_INPUT_*/OUTPUT_*, so do
# not set both. Optional CA_FILE / CLIENT_CERT / CLIENT_KEY, or the INPUT_* / OUTPUT_*
# forms when the certs differ.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=1000}"
: "${CONCURRENCY:=4}"

if [[ "${ES_URL}" == "${DEST_ES_URL}" && "${INDEX}" == "${DEST_INDEX}" ]]; then
  echo "error: source and destination are the same; set DEST_ES_URL or DEST_INDEX (or pass --force if you really mean it)" >&2
  exit 1
fi

warn_auth_file_wins
both_cluster_flags
for t in settings mapping data; do
  EXTRA=()
  if [[ "${t}" == "data" ]]; then
    EXTRA+=(--limit="${LIMIT}" --concurrency="${CONCURRENCY}" --workers=4 --noRefresh --esCompress)
  fi
  rerun_on_4 "${SEARCHDUMP_BIN}" \
    --input="${ES_URL}/${INDEX}" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type="${t}" \
    "${CLUSTER_FLAGS[@]}" \
    "${EXTRA[@]}"
done
echo "copied ${ES_URL}/${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
