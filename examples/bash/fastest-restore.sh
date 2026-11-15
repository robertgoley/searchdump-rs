#!/usr/bin/env bash
# Restore quickly: larger bulk pages, concurrent write batches, and --noRefresh
# so the cluster is not asked to refresh after every batch. --noRefresh is a
# main flag; do not invent --refreshInterval.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

: "${LIMIT:=2000}"
: "${CONCURRENCY:=8}"
: "${WORKERS:=8}"

restore_settings_and_mapping
restore_data_parts \
  --limit="${LIMIT}" \
  --concurrency="${CONCURRENCY}" \
  --workers="${WORKERS}" \
  --bufferSize=32 \
  --noRefresh \
  --esCompress \
  --serverLimits=auto
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
