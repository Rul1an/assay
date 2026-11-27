# shellcheck shell=bash
# Sourced by the pre-push hooks that run a command under a deadline (cargo-clippy.sh and
# scripts/ci/check-linux.sh): one function, so the signal rule is kept in one place.
#
# run_with_deadline SECONDS COMMAND [ARG...]
#
# Runs COMMAND through run-with-timeout.py, next to this file. That wrapper leaves the caller's
# process group, so a terminal's Ctrl+C reaches only the calling script. The wrapper therefore runs
# in the background, and this function passes SIGINT, SIGTERM and SIGHUP on to it. The wrapper
# stops the command's process group within its grace, and the caller then exits 128+N (its EXIT
# trap still runs) instead of going on, or 125 when the wrapper reports that its cleanup failed.
#
# A signal the caller ignores stays ignored, and the caller's own traps for these three signals are
# restored afterwards. Otherwise it returns the wrapper's status: the command's own, 124 at the
# deadline, 125 when the wrapper's cleanup failed.

RUN_WITH_TIMEOUT_PY="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/run-with-timeout.py"

run_with_deadline() {
  local pid status=0 interrupted="" trapped=0 kept saved signame
  saved="$(trap -p INT TERM HUP)"
  python3 "$RUN_WITH_TIMEOUT_PY" "$@" &
  pid=$!
  for signame in INT TERM HUP; do
    if [ "$(trap -p "$signame")" != "trap -- '' SIG$signame" ]; then
      # shellcheck disable=SC2064 # the pid and the signal name are fixed here on purpose
      trap "trapped=1; interrupted=$signame; kill -$signame $pid 2>/dev/null" "$signame"
    fi
  done
  # A trapped signal makes `wait` return 128+N early. If the wrapper is still running, wait again.
  # If it ended meanwhile, bash may already have reaped it: one more `wait` reads the status bash
  # kept for it, and 127 there means the previous `wait` already had the real status.
  while :; do
    trapped=0
    status=0
    wait "$pid" || status=$?
    [ "$trapped" = 1 ] || break
    kill -0 "$pid" 2>/dev/null && continue
    kept=0
    wait "$pid" || kept=$?
    [ "$kept" -eq 127 ] || status=$kept
    break
  done
  trap - INT TERM HUP
  eval "$saved"
  if [ -n "$interrupted" ]; then
    [ "$status" -eq 125 ] && exit 125
    exit $((128 + $(kill -l "$interrupted")))
  fi
  return "$status"
}
