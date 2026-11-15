#!/usr/bin/env bash
# Dump one index to s3://. Set OUTPUT_STORAGE_CONFIG (mounted into the
# container) or SEARCHDUMP_S3_* / AWS_* on the host environment. Never put
# keys in the URL. --overwrite is required to replace an existing object.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

if [[ -z "${OUTPUT_STORAGE_CONFIG}" && -z "${SEARCHDUMP_S3_SECRET_ACCESS_KEY:-}" && -z "${AWS_SECRET_ACCESS_KEY:-}" ]]; then
  echo "error: set OUTPUT_STORAGE_CONFIG or SEARCHDUMP_S3_* / AWS_* credentials" >&2
  exit 1
fi

: "${FILE_SIZE:=1gb}"
: "${LIMIT:=1000}"

rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.settings.json" \
  --type=settings --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.mapping.json" \
  --type=mapping --overwrite
rerun_on_4 run_searchdump input \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.data.json" \
  --type=data --overwrite \
  --compressionType=zstd --compressionLevel=1 \
  --fileSize="${FILE_SIZE}" --limit="${LIMIT}"
echo "wrote ${S3_URI}/${INDEX}.{settings,mapping,data}"
