#!/usr/bin/env bash
# Restore the minimum-resources dump with one write batch at a time.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

EXTRA_FLAGS=(--concurrency=1 --workers=1)
restore_settings_and_mapping
restore_data_parts --limit=100 --concurrency=1 --workers=1 --maxBulkBytes=5mb --serverLimits=min
echo "restored ${INDEX} -> ${DEST_ES_URL}/${DEST_INDEX}"
