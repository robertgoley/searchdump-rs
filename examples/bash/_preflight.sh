# Preflight for the safe-simple examples (1.2): run the given command with
# --serverLimits=check appended and branch on the exit status only (never parse the
# printed "server limits:" line; its wording may change).
#   0  limits read, or a fallback (no monitor privilege, serverless, proxy): go on
#   2  a value you set is above what the cluster can take (printed above), or the command
#      line itself was refused (a usage error, also exit 2; the message above says which)
#   *  the preflight itself failed
# shellcheck shell=bash
preflight() {
  local rc=0
  "$@" --serverLimits=check || rc=$?
  case "${rc}" in
    0) ;;
    2)
      echo "preflight: exit 2: a value you set exceeds what the cluster can take, or the command line was refused; see the message above" >&2
      exit 2
      ;;
    *)
      echo "preflight failed (${rc})" >&2
      exit "${rc}"
      ;;
  esac
}
