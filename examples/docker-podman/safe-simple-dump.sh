#!/usr/bin/env bash
# Safe simple dump of one index inside the image (1.2): settings, mapping and data with NO sizing
# flags; --serverLimits (default auto) sizes the pages from the cluster's limits (min for a
# fragile or shared cluster, off for the 1.1.0 sizes). The preflight (--serverLimits=check) stops
# the script on exit 2. Credentials come from the host environment (passed by name only) or
# HTTP_AUTH_FILE.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"
# shellcheck source=./_preflight.sh
source "${ROOT}/_preflight.sh"

: "${SERVER_LIMITS:=auto}"

preflight run_searchdump input --input="${ES_URL}/${INDEX}" --output=/dev/null --type=data
for t in settings mapping data; do
  ext=json
  [[ "${t}" == data ]] && ext=json.gz
  rerun_on_4 run_searchdump input \
    --input="${ES_URL}/${INDEX}" \
    --output="/data/${INDEX}.${t}.${ext}" \
    --type="${t}" \
    --overwrite \
    --serverLimits="${SERVER_LIMITS}"
done
echo "wrote ${DUMP_DIR}/${INDEX}.{settings,mapping}.json and ${INDEX}.data.json.gz"
