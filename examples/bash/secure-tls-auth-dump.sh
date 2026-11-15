#!/usr/bin/env bash
# Dump over HTTPS with a private CA and optional client certificates.
# Set HTTP_AUTH_FILE (see examples/http-auth.ini.example) or
# SEARCHDUMP_INPUT_USERNAME / SEARCHDUMP_INPUT_PASSWORD, plus CA_FILE,
# and CLIENT_CERT / CLIENT_KEY for mTLS. Do not set --insecure.
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

ensure_dump_dir
dump_settings_and_mapping
input_cluster_flags
rerun_on_4 "${SEARCHDUMP_BIN}" \
  --input="${ES_URL}/${INDEX}" \
  --output="$(data_file json.gz)" \
  --type=data \
  --overwrite \
  --fsCompress \
  --limit=500 \
  "${INPUT_CLUSTER_FLAGS[@]}"
echo "wrote $(data_file json.gz)"
