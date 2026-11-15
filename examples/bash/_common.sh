# Shared defaults and helpers for the host-binary examples.
# Override any of these in the environment before running a script.
# shellcheck shell=bash

: "${ES_URL:=http://127.0.0.1:9200}"
: "${DEST_ES_URL:=${ES_URL}}"
: "${INDEX:=logs}"
: "${DEST_INDEX:=${INDEX}}"
: "${DUMP_DIR:=${PWD}/dumps}"
: "${MATCH:=^logs-}"
: "${HTTP_AUTH_FILE:=}"
: "${CA_FILE:=}"
: "${CLIENT_CERT:=}"
: "${CLIENT_KEY:=}"
: "${INPUT_CA_FILE:=${CA_FILE}}"
: "${OUTPUT_CA_FILE:=${CA_FILE}}"
: "${INPUT_CERT:=${CLIENT_CERT}}"
: "${OUTPUT_CERT:=${CLIENT_CERT}}"
: "${INPUT_KEY:=${CLIENT_KEY}}"
: "${OUTPUT_KEY:=${CLIENT_KEY}}"
: "${INPUT_STORAGE_CONFIG:=}"
: "${OUTPUT_STORAGE_CONFIG:=}"
: "${S3_URI:=s3://backups/${INDEX}}"
: "${SEARCHDUMP_BIN:=searchdump}"
: "${SEARCHDUMP_MULTI_BIN:=searchdump-multi}"

need_bin() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' is not on PATH" >&2
    exit 1
  fi
}

need_bin "${SEARCHDUMP_BIN}"

# Flags for a cluster on the input side (dump source, or restore is from a file).
input_cluster_flags() {
  INPUT_CLUSTER_FLAGS=()
  if [[ -n "${HTTP_AUTH_FILE}" ]]; then
    INPUT_CLUSTER_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
  fi
  if [[ -n "${INPUT_CA_FILE}" ]]; then
    INPUT_CLUSTER_FLAGS+=(--input-ca="${INPUT_CA_FILE}")
  fi
  if [[ -n "${INPUT_CERT}" ]]; then
    INPUT_CLUSTER_FLAGS+=(--input-cert="${INPUT_CERT}")
  fi
  if [[ -n "${INPUT_KEY}" ]]; then
    INPUT_CLUSTER_FLAGS+=(--input-key="${INPUT_KEY}")
  fi
}

# Flags for a cluster on the output side (restore target, or copy destination).
output_cluster_flags() {
  OUTPUT_CLUSTER_FLAGS=()
  if [[ -n "${HTTP_AUTH_FILE}" ]]; then
    OUTPUT_CLUSTER_FLAGS+=(--httpAuthFile="${HTTP_AUTH_FILE}")
  fi
  if [[ -n "${OUTPUT_CA_FILE}" ]]; then
    OUTPUT_CLUSTER_FLAGS+=(--output-ca="${OUTPUT_CA_FILE}")
  fi
  if [[ -n "${OUTPUT_CERT}" ]]; then
    OUTPUT_CLUSTER_FLAGS+=(--output-cert="${OUTPUT_CERT}")
  fi
  if [[ -n "${OUTPUT_KEY}" ]]; then
    OUTPUT_CLUSTER_FLAGS+=(--output-key="${OUTPUT_KEY}")
  fi
}

both_cluster_flags() {
  input_cluster_flags
  output_cluster_flags
  CLUSTER_FLAGS=("${INPUT_CLUSTER_FLAGS[@]}")
  # --httpAuthFile is already in INPUT_CLUSTER_FLAGS; only add output TLS here
  # when the output flags differ. httpAuthFile is shared, so skip duplicating it.
  local f
  for f in "${OUTPUT_CLUSTER_FLAGS[@]}"; do
    if [[ "${f}" == --httpAuthFile=* ]]; then
      continue
    fi
    CLUSTER_FLAGS+=("${f}")
  done
}

ensure_dump_dir() {
  mkdir -p "${DUMP_DIR}"
}

settings_file() { printf '%s/%s.settings.json' "${DUMP_DIR}" "${INDEX}"; }
mapping_file() { printf '%s/%s.mapping.json' "${DUMP_DIR}" "${INDEX}"; }

# Unsplit compressed data path used by the minimum-resource and fastest examples.
data_file() {
  local ext="${1:?extension, e.g. json.gz or json.zst}"
  printf '%s/%s.data.%s' "${DUMP_DIR}" "${INDEX}" "${ext}"
}

# Stem passed as --output when --fileSize/--maxRows will create split parts
# (INDEX.data.split-0.json.gz, ...).
data_stem() {
  printf '%s/%s.data.json' "${DUMP_DIR}" "${INDEX}"
}

# Optional extra searchdump flags for dump_settings_and_mapping / restore_settings_and_mapping.
EXTRA_FLAGS=()

dump_settings_and_mapping() {
  input_cluster_flags
  rerun_on_4 "${SEARCHDUMP_BIN}" \
    --input="${ES_URL}/${INDEX}" \
    --output="$(settings_file)" \
    --type=settings \
    --overwrite \
    "${INPUT_CLUSTER_FLAGS[@]}" \
    "${EXTRA_FLAGS[@]}"
  rerun_on_4 "${SEARCHDUMP_BIN}" \
    --input="${ES_URL}/${INDEX}" \
    --output="$(mapping_file)" \
    --type=mapping \
    --overwrite \
    "${INPUT_CLUSTER_FLAGS[@]}" \
    "${EXTRA_FLAGS[@]}"
}

restore_settings_and_mapping() {
  output_cluster_flags
  "${SEARCHDUMP_BIN}" \
    --input="$(settings_file)" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type=settings \
    "${OUTPUT_CLUSTER_FLAGS[@]}" \
    "${EXTRA_FLAGS[@]}"
  "${SEARCHDUMP_BIN}" \
    --input="$(mapping_file)" \
    --output="${DEST_ES_URL}/${DEST_INDEX}" \
    --type=mapping \
    "${OUTPUT_CLUSTER_FLAGS[@]}" \
    "${EXTRA_FLAGS[@]}"
}

# Restore every split part next to a stem, in numeric order, or the single
# unsplit file if no parts exist.
restore_data_parts() {
  output_cluster_flags
  local stem
  stem="$(data_stem)"
  local -a parts=()
  local f
  shopt -s nullglob
  for f in "${stem%.json}".split-*.json "${stem%.json}".split-*.json.gz "${stem%.json}".split-*.json.zst; do
    parts+=("${f}")
  done
  shopt -u nullglob
  if ((${#parts[@]} == 0)); then
    # unsplitted gzip / zstd / plain
    for f in "$(data_file json.gz)" "$(data_file json.zst)" "${stem}"; do
      if [[ -f "${f}" ]]; then
        parts+=("${f}")
        break
      fi
    done
  fi
  if ((${#parts[@]} == 0)); then
    echo "error: no data file or split parts found under ${DUMP_DIR} for index ${INDEX}" >&2
    exit 1
  fi
  # Sort by the split number when present, otherwise keep the single file.
  local IFS=$'\n'
  # shellcheck disable=SC2207
  parts=($(printf '%s\n' "${parts[@]}" | sort -V))
  unset IFS
  for f in "${parts[@]}"; do
    echo "restoring ${f}"
    "${SEARCHDUMP_BIN}" \
      --input="${f}" \
      --output="${DEST_ES_URL}/${DEST_INDEX}" \
      --type=data \
      "${OUTPUT_CLUSTER_FLAGS[@]}" \
      "${@}"
  done
}

# Exit status 4 (since 1.1.0, L-804b): the run failed after its own retries for a
# transient reason (network, HTTP 429/5xx, a lost scroll context) and nothing was written that a
# rerun would duplicate, so it is safe to run again. rerun_on_4 runs a command and, on exit 4,
# runs it again up to RERUNS times (default 1) after RERUN_DELAY seconds (default 30). Any other
# status is returned as is. With 1.0.0 binaries/images such failures exit 1 and are not rerun.
: "${RERUNS:=1}"
: "${RERUN_DELAY:=30}"
rerun_on_4() {
  local n=0 rc
  while :; do
    rc=0
    "$@" || rc=$?
    if [[ "${rc}" -eq 4 && "${n}" -lt "${RERUNS}" ]]; then
      n=$((n + 1))
      echo "exit 4 (transient, safe to rerun): rerun ${n} of ${RERUNS} in ${RERUN_DELAY}s" >&2
      sleep "${RERUN_DELAY}"
      continue
    fi
    return "${rc}"
  done
}

# Optional --restartOnContextLoss=N (since 1.1.0): a scroll dump into a file restarts
# from the start when the cluster loses its scroll context. Not for --searchAfter --pit, and not
# accepted by 1.0.0 binaries/images, so it is off unless RESTART_ON_CONTEXT_LOSS is set.
: "${RESTART_ON_CONTEXT_LOSS:=}"
restart_flags() {
  RESTART_FLAGS=()
  if [[ -n "${RESTART_ON_CONTEXT_LOSS}" ]]; then
    RESTART_FLAGS+=(--restartOnContextLoss="${RESTART_ON_CONTEXT_LOSS}")
  fi
}

# --httpAuthFile wins (since 1.1.0, L-645b): when it is set, its user/password is used for
# both directions, and URL credentials, --input-*/--output-* user flags and SEARCHDUMP_*_USERNAME/
# _PASSWORD are ignored (with a warning). Say so here before the run.
warn_auth_file_wins() {
  if [[ -n "${HTTP_AUTH_FILE}" && ( -n "${SEARCHDUMP_INPUT_USERNAME:-}" || -n "${SEARCHDUMP_OUTPUT_USERNAME:-}" ) ]]; then
    echo "note: HTTP_AUTH_FILE is set, so its user is used for both clusters; SEARCHDUMP_INPUT_*/SEARCHDUMP_OUTPUT_* are ignored (unset HTTP_AUTH_FILE to use different users)" >&2
  fi
}
