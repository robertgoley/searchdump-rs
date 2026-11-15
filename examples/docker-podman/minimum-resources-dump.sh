#!/usr/bin/env bash
# Dump one index with small pages inside the dhi.io searchdump image.
# See examples/README.md for the searchdump-multi small-heap 429 note
# (bitnamilegacy Elasticsearch): its --limit=5000 / --maxResponseMb=0 is unsafe
# on a small heap; this script uses searchdump's own --limit=100 instead.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.settings.json" \
  --type=settings \
  --overwrite \
  --limit=100 --workers=1 --concurrency=1 --maxResponseMb=64
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.mapping.json" \
  --type=mapping \
  --overwrite \
  --limit=100 --workers=1 --concurrency=1 --maxResponseMb=64
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.data.json.gz" \
  --type=data \
  --overwrite \
  --fsCompress \
  --limit=100 --workers=1 --concurrency=1 --maxResponseMb=64
echo "wrote ${DUMP_DIR}/${INDEX}.{settings,mapping}.json and ${INDEX}.data.json.gz"
