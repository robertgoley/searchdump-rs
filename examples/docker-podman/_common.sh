# Shared defaults and helpers for the container examples (podman or docker).
# shellcheck shell=bash

: "${CONTAINER_ENGINE:=podman}"
: "${SEARCHDUMP_IMAGE:=ghcr.io/robertgoley/searchdump-rs:1.3.0-dhi-debian}"
: "${NETWORK:=host}"
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
: "${SSL_CERT_FILE_HOST:=}"
# Container user. The image defaults to 65532; override with e.g.
# CONTAINER_USER="$(id -u):$(id -g)" if you cannot chown the dump directory.
# Do not use USER: that is the host login name and would break --user.
: "${CONTAINER_USER:=65532:65532}"

need_bin() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' is not on PATH" >&2
    exit 1
  fi
}

need_bin "${CONTAINER_ENGINE}"

ensure_dump_dir() {
  mkdir -p "${DUMP_DIR}"
  if [[ "${CONTAINER_USER}" == "65532:65532" ]] && [[ ! -w "${DUMP_DIR}" || "$(stat -c %u "${DUMP_DIR}" 2>/dev/null || true)" != "65532" ]]; then
    echo "note: the image runs as uid 65532; '${DUMP_DIR}' must be writable by that user." >&2
    echo "      for example: sudo chown -R 65532:65532 '${DUMP_DIR}'" >&2
    echo "      or rerun with CONTAINER_USER=\"\$(id -u):\$(id -g)\" to run the container as yourself." >&2
  fi
}

# Volume suffix: :Z for podman on SELinux hosts; docker usually does not want it.
volume() {
  local host="$1" dest="$2" mode="${3:-}"
  local spec="${host}:${dest}"
  if [[ -n "${mode}" ]]; then
    spec="${spec}:${mode}"
  fi
  if [[ "${CONTAINER_ENGINE}" == "podman" ]]; then
    if [[ -n "${mode}" ]]; then
      spec="${spec},Z"
    else
      spec="${spec}:Z"
    fi
  fi
  printf '%s' "${spec}"
}

# Build a `podman/docker run` command into RUN_CMD, then append image + args.
prepare_run() {
  RUN_CMD=("${CONTAINER_ENGINE}" run --rm)
  if [[ -n "${NETWORK}" ]]; then
    RUN_CMD+=(--network "${NETWORK}")
  fi
  if [[ -n "${CONTAINER_USER}" ]]; then
    RUN_CMD+=(--user "${CONTAINER_USER}")
  fi
  ensure_dump_dir
  RUN_CMD+=(-v "$(volume "${DUMP_DIR}" /data)")
  # Non-secret run settings (always passed so /bin/sh loops inside the image can use them).
  RUN_CMD+=(
    -e "ES_URL=${ES_URL}"
    -e "DEST_ES_URL=${DEST_ES_URL}"
    -e "INDEX=${INDEX}"
    -e "DEST_INDEX=${DEST_INDEX}"
  )

  # Pass credentials through from the host environment when they are set (no values in argv).
  local ev
  for ev in \
    SEARCHDUMP_INPUT_USERNAME SEARCHDUMP_INPUT_PASSWORD \
    SEARCHDUMP_OUTPUT_USERNAME SEARCHDUMP_OUTPUT_PASSWORD \
    SEARCHDUMP_S3_ENDPOINT SEARCHDUMP_S3_REGION \
    SEARCHDUMP_S3_ACCESS_KEY_ID SEARCHDUMP_S3_SECRET_ACCESS_KEY \
    AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN \
    AWS_REGION AWS_DEFAULT_REGION AWS_ENDPOINT_URL AWS_ENDPOINT_URL_S3 \
    SSL_CERT_FILE; do
    if [[ -n "${!ev:-}" ]]; then
      RUN_CMD+=(-e "${ev}")
    fi
  done

  if [[ -n "${HTTP_AUTH_FILE}" ]]; then
    RUN_CMD+=(-v "$(volume "${HTTP_AUTH_FILE}" /run/searchdump/http-auth.ini ro)")
  fi
  if [[ -n "${INPUT_STORAGE_CONFIG}" ]]; then
    RUN_CMD+=(-v "$(volume "${INPUT_STORAGE_CONFIG}" /run/searchdump/input-storage.ini ro)")
  fi
  if [[ -n "${OUTPUT_STORAGE_CONFIG}" ]]; then
    RUN_CMD+=(-v "$(volume "${OUTPUT_STORAGE_CONFIG}" /run/searchdump/output-storage.ini ro)")
  fi
  if [[ -n "${INPUT_CA_FILE}" ]]; then
    RUN_CMD+=(-v "$(volume "${INPUT_CA_FILE}" /run/searchdump/input-ca.pem ro)")
  fi
  if [[ -n "${OUTPUT_CA_FILE}" ]]; then
    RUN_CMD+=(-v "$(volume "${OUTPUT_CA_FILE}" /run/searchdump/output-ca.pem ro)")
  fi
  if [[ -n "${INPUT_CERT}" ]]; then
    RUN_CMD+=(-v "$(volume "${INPUT_CERT}" /run/searchdump/input-cert.pem ro)")
  fi
  if [[ -n "${OUTPUT_CERT}" ]]; then
    RUN_CMD+=(-v "$(volume "${OUTPUT_CERT}" /run/searchdump/output-cert.pem ro)")
  fi
  if [[ -n "${INPUT_KEY}" ]]; then
    RUN_CMD+=(-v "$(volume "${INPUT_KEY}" /run/searchdump/input-key.pem ro)")
  fi
  if [[ -n "${OUTPUT_KEY}" ]]; then
    RUN_CMD+=(-v "$(volume "${OUTPUT_KEY}" /run/searchdump/output-key.pem ro)")
  fi
  if [[ -n "${SSL_CERT_FILE_HOST}" ]]; then
    RUN_CMD+=(-v "$(volume "${SSL_CERT_FILE_HOST}" /run/searchdump/ca-bundle.pem ro)")
    RUN_CMD+=(-e SSL_CERT_FILE=/run/searchdump/ca-bundle.pem)
  fi
}

# Extra searchdump flags derived from mounted files (cluster side named by $1 = input|output|both).
mounted_flags() {
  local side="$1"
  MOUNTED_FLAGS=()
  if [[ -n "${HTTP_AUTH_FILE}" ]]; then
    MOUNTED_FLAGS+=(--httpAuthFile=/run/searchdump/http-auth.ini)
  fi
  if [[ -n "${INPUT_STORAGE_CONFIG}" ]]; then
    MOUNTED_FLAGS+=(--inputStorageConfig=/run/searchdump/input-storage.ini)
  fi
  if [[ -n "${OUTPUT_STORAGE_CONFIG}" ]]; then
    MOUNTED_FLAGS+=(--outputStorageConfig=/run/searchdump/output-storage.ini)
  fi
  # searchdump-multi has --httpAuthFile and the storage-config flags, but no
  # --ca/--cert/--key. A private CA must be provided via SSL_CERT_FILE_HOST.
  if [[ "${side}" == "multi" ]]; then
    return 0
  fi
  if [[ "${side}" == "input" || "${side}" == "both" ]]; then
    if [[ -n "${INPUT_CA_FILE}" ]]; then
      MOUNTED_FLAGS+=(--input-ca=/run/searchdump/input-ca.pem)
    fi
    if [[ -n "${INPUT_CERT}" ]]; then
      MOUNTED_FLAGS+=(--input-cert=/run/searchdump/input-cert.pem)
    fi
    if [[ -n "${INPUT_KEY}" ]]; then
      MOUNTED_FLAGS+=(--input-key=/run/searchdump/input-key.pem)
    fi
  fi
  if [[ "${side}" == "output" || "${side}" == "both" ]]; then
    if [[ -n "${OUTPUT_CA_FILE}" ]]; then
      MOUNTED_FLAGS+=(--output-ca=/run/searchdump/output-ca.pem)
    fi
    if [[ -n "${OUTPUT_CERT}" ]]; then
      MOUNTED_FLAGS+=(--output-cert=/run/searchdump/output-cert.pem)
    fi
    if [[ -n "${OUTPUT_KEY}" ]]; then
      MOUNTED_FLAGS+=(--output-key=/run/searchdump/output-key.pem)
    fi
  fi
}

run_searchdump() {
  prepare_run
  mounted_flags "$1"
  shift
  "${RUN_CMD[@]}" "${SEARCHDUMP_IMAGE}" "${MOUNTED_FLAGS[@]}" "${@}"
}

run_searchdump_multi() {
  prepare_run
  mounted_flags "$1"
  shift
  "${RUN_CMD[@]}" --entrypoint searchdump-multi "${SEARCHDUMP_IMAGE}" "${MOUNTED_FLAGS[@]}" "${@}"
}

# Run a short shell snippet inside the image (settings+mapping+data loops, split restores).
run_sh() {
  prepare_run
  local script="$1"
  "${RUN_CMD[@]}" --entrypoint /bin/sh "${SEARCHDUMP_IMAGE}" -ceu "${script}"
}

# Paths inside the container.
c_settings() { printf '/data/%s.settings.json' "${INDEX}"; }
c_mapping() { printf '/data/%s.mapping.json' "${INDEX}"; }
c_data() {
  local ext="${1:?}"
  printf '/data/%s.data.%s' "${INDEX}" "${ext}"
}
c_stem() { printf '/data/%s.data.json' "${INDEX}"; }

# Quote flags so they can be embedded in a /bin/sh -c script.
quoted_flags() {
  local out="" a
  for a in "$@"; do
    out+=" $(printf '%q' "${a}")"
  done
  printf '%s' "${out}"
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
