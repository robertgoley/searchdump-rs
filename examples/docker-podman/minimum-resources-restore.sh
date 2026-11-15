#!/usr/bin/env bash
# Restore the minimum-resources dump with one write batch at a time.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

run_searchdump output \
  --input="/data/${INDEX}.settings.json" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=settings \
  --concurrency=1 --workers=1
run_searchdump output \
  --input="/data/${INDEX}.mapping.json" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=mapping \
  --concurrency=1 --workers=1
run_searchdump output \
  --input="/data/${INDEX}.data.json.gz" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=data \
  --limit=100 --concurrency=1 --workers=1 --maxBulkBytes=5mb --serverLimits=min
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
