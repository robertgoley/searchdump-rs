#!/usr/bin/env bash
# Fast restore: larger bulks, concurrent writes, --noRefresh (a main flag).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=2000}"
: "${CONCURRENCY:=8}"
: "${WORKERS:=8}"

run_searchdump output \
  --input="/data/${INDEX}.settings.json" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=settings
run_searchdump output \
  --input="/data/${INDEX}.mapping.json" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=mapping
run_searchdump output \
  --input="/data/${INDEX}.data.json.zst" \
  --output="${DEST_ES_URL}/${DEST_INDEX}" \
  --type=data \
  --limit="${LIMIT}" --concurrency="${CONCURRENCY}" --workers="${WORKERS}" \
  --bufferSize=32 --noRefresh --esCompress --serverLimits=auto
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
