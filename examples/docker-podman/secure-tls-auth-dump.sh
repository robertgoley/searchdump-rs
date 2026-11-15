#!/usr/bin/env bash
# Dump over HTTPS with a private CA and optional client certificates.
# Set HTTP_AUTH_FILE or SEARCHDUMP_INPUT_USERNAME / SEARCHDUMP_INPUT_PASSWORD,
# plus CA_FILE (PEM), and CLIENT_CERT / CLIENT_KEY for mTLS.
# Do not set --insecure. The files are mounted at /run/searchdump/.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

if [[ -z "${HTTP_AUTH_FILE}" && -z "${SEARCHDUMP_INPUT_USERNAME:-}" ]]; then
  echo "error: set HTTP_AUTH_FILE or SEARCHDUMP_INPUT_USERNAME / SEARCHDUMP_INPUT_PASSWORD" >&2
  exit 1
fi
if [[ -z "${CA_FILE}" && -z "${INPUT_CA_FILE}" ]]; then
  echo "error: set CA_FILE or INPUT_CA_FILE to your cluster CA (PEM)" >&2
  exit 1
fi

rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.settings.json" \
  --type=settings --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.mapping.json" \
  --type=mapping --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="/data/${INDEX}.data.json.gz" \
  --type=data --overwrite --fsCompress --limit=500
echo "wrote ${DUMP_DIR}/${INDEX}.data.json.gz"
