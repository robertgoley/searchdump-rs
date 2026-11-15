#!/usr/bin/env bash
# Dump a large index in --fileSize parts (default 1gb) with a longer scroll
# keep-alive. searchdump has no --resume; retry the index with --overwrite
# if the run fails. For many indices, use multi-index-dump.sh --resume.
# Exit 4 (transient) is rerun once (RERUNS). RESTART_ON_CONTEXT_LOSS=N adds
# --restartOnContextLoss=N: only with an image built from main after 1.0.0
# (make image); the published 1.0.0 image rejects the flag.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${FILE_SIZE:=1gb}"
: "${SCROLL_TIME:=30m}"
: "${LIMIT:=1000}"
: "${WORKERS:=4}"

rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.settings.json" \
  --type=settings --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.mapping.json" \
  --type=mapping --overwrite
restart_flags
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.data.json" \
  --type=data --overwrite \
  --compressionType=zstd --compressionLevel=1 \
  --fileSize="${FILE_SIZE}" --scrollTime="${SCROLL_TIME}" \
  --limit="${LIMIT}" --workers="${WORKERS}" --maxResponseMb=256 \
  "${RESTART_FLAGS[@]}"
echo "wrote split parts next to ${DUMP_DIR}/${INDEX}.data.json"
