#!/usr/bin/env bash
# Safe simple dump of one index (1.2): settings, mapping and data with NO sizing flags.
# --serverLimits (default auto; written out here so it is visible) reads the cluster's heap,
# breakers and limits once and sizes the pages itself; SERVER_LIMITS=min halves the budget for a
# fragile or shared cluster, SERVER_LIMITS=off restores the 1.1.0 sizes. A preflight with
# --serverLimits=check prints what it found and stops the script on exit 2.
#
#   ES_URL=https://src:9200 INDEX=logs DUMP_DIR=/data ./safe-simple-dump.sh
#
# Credentials: SEARCHDUMP_INPUT_USERNAME / SEARCHDUMP_INPUT_PASSWORD in the environment, or
# HTTP_AUTH_FILE (an INI file, chmod 600); never on the command line.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

ensure_dump_dir
input_cluster_flags
preflight "${SEARCHDUMP_BIN}" --input="${ES_URL}/${INDEX}" --output=/dev/null --type=data \
  "${INPUT_CLUSTER_FLAGS[@]}"
for t in settings mapping data; do
  ext=json
  [[ "${t}" == data ]] && ext=json.gz
  rerun_on_4 "${SEARCHDUMP_BIN}" \
    --input="${ES_URL}/${INDEX}" \
    --output="${DUMP_DIR}/${INDEX}.${t}.${ext}" \
    --type="${t}" \
    --overwrite \
    --serverLimits="${SERVER_LIMITS}" \
    "${INPUT_CLUSTER_FLAGS[@]}"
done
echo "wrote ${DUMP_DIR}/${INDEX}.{settings,mapping}.json and ${INDEX}.data.json.gz"
