#!/usr/bin/env bash
# Dump one index to s3:// (MinIO and other S3-compatible endpoints work the
# same). Credentials go in OUTPUT_STORAGE_CONFIG (see
# examples/storage-config.ini.example) or SEARCHDUMP_S3_* / AWS_* — never in
# the URL. --overwrite is required to replace an existing object.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_common.sh
source "${ROOT}/_common.sh"

if [[ -z "${OUTPUT_STORAGE_CONFIG}" && -z "${SEARCHDUMP_S3_SECRET_ACCESS_KEY:-}" && -z "${AWS_SECRET_ACCESS_KEY:-}" ]]; then
  echo "error: set OUTPUT_STORAGE_CONFIG or SEARCHDUMP_S3_* / AWS_* credentials" >&2
  exit 1
fi

input_cluster_flags
STORAGE_FLAGS=()
if [[ -n "${OUTPUT_STORAGE_CONFIG}" ]]; then
  STORAGE_FLAGS+=(--outputStorageConfig="${OUTPUT_STORAGE_CONFIG}")
fi

rerun_on_4 "${SEARCHDUMP_BIN}" \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.settings.json" \
  --type=settings \
  --overwrite \
  "${INPUT_CLUSTER_FLAGS[@]}" \
  "${STORAGE_FLAGS[@]}"
rerun_on_4 "${SEARCHDUMP_BIN}" \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.mapping.json" \
  --type=mapping \
  --overwrite \
  "${INPUT_CLUSTER_FLAGS[@]}" \
  "${STORAGE_FLAGS[@]}"
rerun_on_4 "${SEARCHDUMP_BIN}" \
  --input="${ES_URL}/${INDEX}" \
  --output="${S3_URI}/${INDEX}.data.json" \
  --type=data \
  --overwrite \
  --compressionType=zstd \
  --compressionLevel=1 \
  --fileSize="${FILE_SIZE:-1gb}" \
  --limit="${LIMIT:-1000}" \
  "${INPUT_CLUSTER_FLAGS[@]}" \
  "${STORAGE_FLAGS[@]}"
echo "wrote ${S3_URI}/${INDEX}.{settings,mapping,data} (data may be split parts)"
