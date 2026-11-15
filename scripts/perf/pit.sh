#!/usr/bin/env bash
# Point-in-time perf for the 1.1 release (project/plan/ROADMAP-1.1-PLAN.md, "Point-in-time test
# plan"): build PIT_BASE (v1.0.0-beta1) and every later commit that changes Rust code, then run the
# quick engine trio against each build, engine by engine, so neighbouring steps are measured close
# together in time. Every build and every (build, engine) run leaves a marker under PIT_STATE, so
# the same command resumes where it stopped (a STOP file in PIT_STATE stops it between runs).
#
#   scripts/perf/pit.sh list        steps (label, commit, perf host id), written to PIT_STATE/steps.tsv
#   scripts/perf/pit.sh check       cargo check --all-targets at every step (stops at the first error)
#   scripts/perf/pit.sh build       release binaries into PIT_BIN_ROOT/<shorthash>/
#   scripts/perf/pit.sh run         the perf suite per engine and step (needs PIT_ENV, see below)
#   scripts/perf/pit.sh aggregate   step-by-step table: PIT_STATE/pit-report.md
#   scripts/perf/pit.sh all         build, run, aggregate
#
# The harness (suite.py, measure.py with the 1.1-5 wait4 timer, the case list) always comes from
# THIS checkout; only the binaries change between steps. Run it from the release/1.1 head.
#
# Environment (defaults in brackets):
#   PIT_ENV        file sourced before `run`: the isolated podman setup (XDG_RUNTIME_DIR, PATH,
#                  PERF_ENGINE_PORT, NODE_ED_BIN). PERF_BIN_DIR/PERF_HOST_ID are set here. [none]
#   PIT_STATE      markers, logs, report [/workspace/perf-pit-1.1]
#   PIT_BIN_ROOT   binaries per step [/workspace/perf-bin-1.1]
#   PIT_SRC_ROOT   temporary build worktrees [/workspace/pit-src]
#   PIT_TARGET     shared CARGO_TARGET_DIR (dependencies build once) [/workspace/pit-target]
#   PIT_BASE       first step [v1.0.0-beta1]        PIT_HEAD   last commit [HEAD]
#   PIT_ENGINES    ["es710-plain es819-plain os3-plain"]       PIT_REPS [3]
#   PIT_NODE       which steps also measure Node: base | all | none [base]
#   PIT_AB         1 adds the 1.1 A/B cases (restore-norefresh, copy-norefresh) [1]
#   PIT_HOST       perf host id prefix [grokbox]: base -> <prefix>-beta1-timer2,
#                  commit -> <prefix>-1.1-<shorthash>
#   PIT_TOOLCHAIN  [1.91.1]   PIT_PROFILE [release]   PIT_JOBS [2]   PIT_NICE [10]
#   PIT_REAP_CONMON / PIT_REAP_PASTA  optional pgrep -f patterns of orphaned conmon / pasta
#                  processes of THIS isolated store to kill before each run (never set them to
#                  something that matches another driver's processes) [unset: no reaping]
set -uo pipefail
umask 077
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root" || exit 2

STATE="${PIT_STATE:-/workspace/perf-pit-1.1}"
BIN_ROOT="${PIT_BIN_ROOT:-/workspace/perf-bin-1.1}"
SRC_ROOT="${PIT_SRC_ROOT:-/workspace/pit-src}"
TARGET="${PIT_TARGET:-/workspace/pit-target}"
BASE="${PIT_BASE:-v1.0.0-beta1}"
HEAD_REF="${PIT_HEAD:-HEAD}"
ENGINES="${PIT_ENGINES:-es710-plain es819-plain os3-plain}"
REPS="${PIT_REPS:-3}"
NODE_STEPS="${PIT_NODE:-base}"
AB="${PIT_AB:-1}"
HOST_PREFIX="${PIT_HOST:-grokbox}"
TOOLCHAIN="${PIT_TOOLCHAIN:-1.91.1}"
PROFILE="${PIT_PROFILE:-release}"
JOBS="${PIT_JOBS:-2}"
NICE="${PIT_NICE:-10}"
RUST_PATHS=(src tools Cargo.toml Cargo.lock build.rs)
BINS=(searchdump searchdump-multi edgen edverify)

log() { echo "$(date '+%F %T') $*" | tee -a "$STATE/summary.txt" >&2; }
die() { log "STOP: $*"; exit "${2:-1}"; }


# steps.tsv: index, label, short hash, full hash, perf host id, subject
write_steps() {
  local base_full base_short i=0 h
  base_full=$(git rev-parse --verify "$BASE^{commit}") || die "no commit $BASE" 2
  git merge-base --is-ancestor "$base_full" "$HEAD_REF" || die "$BASE is not an ancestor of $HEAD_REF" 2
  base_short=$(git rev-parse --short "$base_full")
  {
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' 0 beta1 "$base_short" "$base_full" "$HOST_PREFIX-beta1-timer2" \
      "$(git log -1 --format=%s "$base_full")"
    for h in $(git rev-list --reverse "$base_full..$HEAD_REF" -- "${RUST_PATHS[@]}"); do
      i=$((i + 1))
      local s subj label
      s=$(git rev-parse --short "$h")
      subj=$(git log -1 --format=%s "$h")
      label=$(grep -oE '1\.1-[0-9]+' <<<"$subj" | head -1)
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$i" "${label:-$s}" "$s" "$h" "$HOST_PREFIX-1.1-$s" "$subj"
    done
  } > "$STATE/steps.tsv.new"
  mv "$STATE/steps.tsv.new" "$STATE/steps.tsv"
}

steps() { cut -f1-5 "$STATE/steps.tsv"; }

worktree() { # $1 short, $2 full -> path of a detached worktree at that commit
  local wt="$SRC_ROOT/$1"
  if [[ ! -d "$wt" ]]; then
    mkdir -p "$SRC_ROOT"
    git worktree add --detach --quiet "$wt" "$2" >&2 || die "git worktree add $wt failed" 2
  fi
  echo "$wt"
}

drop_worktree() { git worktree remove --force "$SRC_ROOT/$1" 2>/dev/null || true; }

cargo_in() { # $1 worktree, rest: cargo args
  local wt="$1"; shift
  (cd "$wt" && CARGO_TARGET_DIR="$TARGET" nice -n "$NICE" cargo "+$TOOLCHAIN" "$@")
}

cmd_check() {
  local idx label short full host
  while IFS=$'\t' read -r -u 3 idx label short full host; do
    [[ -e "$STATE/check/$short.ok" ]] && continue
    local wt; wt=$(worktree "$short" "$full") || exit 2
    log "check $label $short"
    if ! cargo_in "$wt" check --all-targets --locked -p searchdump-rs -j "$JOBS" \
        > "$STATE/logs/check-$short.log" 2>&1; then
      die "cargo check failed at $label $short (log: $STATE/logs/check-$short.log). Fix it with a 'git commit --fixup=$short', fold it in with 'git rebase -i --autosquash $BASE' (local branch only), then rerun list + check" 1
    fi
    touch "$STATE/check/$short.ok"
    drop_worktree "$short"
  done 3< <(steps)
  log "check: every step compiles"
}

profile_dir() { [[ "$PROFILE" == dev ]] && echo debug || echo "$PROFILE"; }

cmd_build() {
  local idx label short full host
  while IFS=$'\t' read -r -u 3 idx label short full host; do
    local out="$BIN_ROOT/$short"
    [[ -e "$out/.complete" ]] && continue
    local wt; wt=$(worktree "$short" "$full") || exit 2
    log "build $label $short"
    if ! cargo_in "$wt" build --profile "$PROFILE" --locked --bins -p searchdump-rs -p edgen -p edverify \
        -j "$JOBS" > "$STATE/logs/build-$short.log" 2>&1; then
      die "build failed at $label $short (log: $STATE/logs/build-$short.log)" 1
    fi
    mkdir -p "$out"
    local b
    for b in "${BINS[@]}"; do
      cp -f "$TARGET/$(profile_dir)/$b" "$out/$b" || die "missing $b after building $short" 1
    done
    {
      echo "commit=$full"
      echo "label=$label"
      echo "built=$(date '+%F %T %Z')"
      echo "profile=$PROFILE toolchain=$(cargo "+$TOOLCHAIN" --version 2>/dev/null)"
      (cd "$out" && sha256sum "${BINS[@]}")
    } > "$out/.complete"
    drop_worktree "$short"
  done 3< <(steps)
  log "build: all binaries in $BIN_ROOT"
}

cmd_run() {
  if [[ -n "${PIT_ENV:-}" ]]; then
    # shellcheck source=/dev/null
    . "$PIT_ENV" || die "cannot source PIT_ENV" 2
  fi
  : "${PERF_ENGINE_PORT:?set PERF_ENGINE_PORT (PIT_ENV)}"
  local harness; harness=$(git rev-parse --short HEAD)
  local e idx label short full host
  for e in $ENGINES; do
    while IFS=$'\t' read -r -u 3 idx label short full host; do
      local mark="$STATE/runs/$short/$e.done"
      [[ -e "$mark" ]] && continue
      [[ -e "$STATE/STOP" ]] && { log "STOP file present"; exit 0; }
      [[ -e "$BIN_ROOT/$short/.complete" ]] || die "no binaries for $label $short; run build first" 2
      local free_gb; free_gb=$(df --output=avail -BG / | tail -1 | tr -dc 0-9)
      (( free_gb < 8 )) && die "disk: ${free_gb}G free before $e/$short" 3
      # leftovers of this driver's own perf containers (the store PIT_ENV selects)
      podman ps -a --format '{{.Names}}' | grep -E -- '-perf-[0-9]+-' | xargs -r podman rm -f >/dev/null 2>&1
      podman volume prune -f >/dev/null 2>&1
      if [[ -n "${PIT_REAP_CONMON:-}" ]]; then
        local pid
        for pid in $(pgrep -f "$PIT_REAP_CONMON"); do pkill -KILL -P "$pid" 2>/dev/null; kill -KILL "$pid" 2>/dev/null; done
      fi
      [[ -n "${PIT_REAP_PASTA:-}" ]] && pkill -KILL -f "$PIT_REAP_PASTA" 2>/dev/null
      sleep 2
      ss -ltn | grep -q "127.0.0.1:${PERF_ENGINE_PORT} " && die "port $PERF_ENGINE_PORT busy before $e/$short" 5
      local n=0
      until (( $(awk '/MemAvailable/{print int($2/1024)}' /proc/meminfo) >= 4500 )) || (( n > 60 )); do
        sleep 10; n=$((n + 1))
      done
      local skip=()
      case "$NODE_STEPS" in
        all) ;;
        none) skip=(--skip-node) ;;
        *) [[ "$idx" == 0 ]] || skip=(--skip-node) ;;
      esac
      log "START $e $label $short host=$host harness=$harness ${skip[*]}"
      local t0=$SECONDS rc lf="$STATE/logs/run-$short-$e.log"
      PERF_BIN_DIR="$BIN_ROOT/$short" PERF_HOST_ID="$host" PERF_AB_CASES="$AB" \
        bash scripts/perf/run-suite.sh --host-tools --tier L0 --place box --reps "$REPS" --engines "$e" \
        --baseline-dir results/perf/pit-baselines "${skip[@]}" > "$lf" 2>&1
      rc=$?
      local d; d=$(grep -o "$root/results/perf/[^ /]*" "$lf" | tail -1)
      log "$e $label $short rc=$rc $(( (SECONDS - t0) / 60 ))min $(grep -o 'PERF-SUMMARY.*' "$lf" | tail -1)"
      (( rc != 0 && SECONDS - t0 < 30 )) && die "fast failure at $e/$short (log: $lf)" 4
      [[ -n "$d" ]] && find "$d" -type d -name work -prune -exec rm -rf {} + 2>/dev/null
      if [[ -z "$d" || ! -e "$d/combined.json" ]]; then log "NO-RESULT $e $short (not marked done)"; continue; fi
      mkdir -p "$STATE/runs/$short"
      printf 'rc=%s dir=%s harness=%s\n' "$rc" "$d" "$harness" > "$mark"
    done 3< <(steps)
  done
  log "run: all engines and steps done"
}

cmd_aggregate() {
  python3 scripts/perf/pit_table.py --state "$STATE" > "$STATE/pit-report.md.new" &&
    mv "$STATE/pit-report.md.new" "$STATE/pit-report.md" && log "report: $STATE/pit-report.md"
}

case "${1:-}" in
  list | check | build | run | aggregate | all) mkdir -p "$STATE/logs" "$STATE/runs" "$STATE/check" ;;
esac
case "${1:-}" in
  list) write_steps; awk -F'\t' '{printf "%-2s %-6s %-9s %-26s %s\n", $1, $2, $3, $5, $6}' "$STATE/steps.tsv" ;;
  check) write_steps; cmd_check ;;
  build) write_steps; cmd_build ;;
  run) [[ -e "$STATE/steps.tsv" ]] || write_steps; cmd_run ;;
  aggregate) cmd_aggregate ;;
  all) write_steps; cmd_build && cmd_run && cmd_aggregate ;;
  *) sed -n '2,/^set -uo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
