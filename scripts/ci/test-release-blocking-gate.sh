#!/usr/bin/env bash
# Every case here is a defect that has already happened, or one that would pass silently today.
#
# The gate this tests decides whether a release may proceed. Before the extraction in #1993 it was
# inline bash in `release.yml`, so its only test was cutting a release, and it failed open three
# times in one day -- each time logging like success. The `$GH_BIN` seam exists so the two cases
# that had no other way to be reached (a milestone listing that fails, and a milestone past the
# first page) can be driven here.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/release_blocking_gate.sh"
REPO="Rul1an/assay"
TEST_TEMP_DIR=""
FAILURES=0

cleanup() { [ -n "$TEST_TEMP_DIR" ] && rm -rf "$TEST_TEMP_DIR"; }
trap cleanup EXIT

# A `gh` that answers from files, and honours `--paginate` the way the real one does.
#
# The pagination behaviour is the point: without `--paginate` it returns only the first page, so a
# gate that drops the flag finds nothing and opens. That is defect 2 from #1993, and it is
# reachable here and nowhere else.
make_fake_gh() {
  cat > "$1" <<'EOF'
#!/usr/bin/env bash
set -uo pipefail
printf '%s\n' "$*" >> "$FAKE_GH_LOG"

case "$1" in
  api)
    [ "${FAKE_MILESTONES_FAIL:-}" = "1" ] && { echo "API rate limit exceeded" >&2; exit 1; }
    if printf '%s\n' "$@" | grep -qx -- '--paginate'; then
      cat "$FAKE_MILESTONES_ALL"
    else
      head -n "${FAKE_PAGE_SIZE:-100}" "$FAKE_MILESTONES_ALL"
    fi
    ;;
  issue)
    [ "${FAKE_ISSUES_FAIL:-}" = "1" ] && { echo "API rate limit exceeded" >&2; exit 1; }
    cat "$FAKE_BLOCKERS"
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 1
    ;;
esac
EOF
  chmod +x "$1"
}

# Runs the gate and reports the exit code plus combined output.
run_gate() {
  local version="$1"
  set +e
  GATE_OUT="$(GH_BIN="$TEST_TEMP_DIR/gh" \
    FAKE_GH_LOG="$TEST_TEMP_DIR/gh.log" \
    FAKE_MILESTONES_ALL="$TEST_TEMP_DIR/milestones.txt" \
    FAKE_BLOCKERS="$TEST_TEMP_DIR/blockers.txt" \
    FAKE_MILESTONES_FAIL="${FAKE_MILESTONES_FAIL:-}" \
    FAKE_ISSUES_FAIL="${FAKE_ISSUES_FAIL:-}" \
    FAKE_PAGE_SIZE="${FAKE_PAGE_SIZE:-100}" \
    GITHUB_ACTIONS="${GITHUB_ACTIONS_OVERRIDE:-}" \
    bash "$SCRIPT" "$version" "$REPO" 2>&1)"
  GATE_STATUS=$?
  set -e
}

check() {
  local name="$1" want_status="$2" want_text="${3:-}"
  if [ "$GATE_STATUS" != "$want_status" ]; then
    echo "FAIL  $name: exit $GATE_STATUS, wanted $want_status"
    echo "      output: $GATE_OUT"
    FAILURES=$((FAILURES + 1))
    return
  fi
  if [ -n "$want_text" ] && ! grep -qF -- "$want_text" <<<"$GATE_OUT"; then
    echo "FAIL  $name: output does not contain '$want_text'"
    echo "      output: $GATE_OUT"
    FAILURES=$((FAILURES + 1))
    return
  fi
  echo "ok    $name"
}

TEST_TEMP_DIR="$(mktemp -d)"
make_fake_gh "$TEST_TEMP_DIR/gh"
printf 'v3.37.0\nv3.38.0\n' > "$TEST_TEMP_DIR/milestones.txt"
: > "$TEST_TEMP_DIR/blockers.txt"

# --- Both version shapes resolve to one milestone -------------------------------------------
#
# Defect 3: `resolve-release-version.sh` returns the tag form, the gate prefixed another `v`, and
# `vv3.38.0` matched nothing -- so the gate opened. The query logic had been verified twice against
# live state; what was never checked was what the variable held.
run_gate "v3.38.0"
check "tag form v3.38.0 finds its milestone" 0 "no open release-blocking issues"
run_gate "3.38.0"
check "bare form 3.38.0 finds the same milestone" 0 "no open release-blocking issues"

# --- An unrecognised version refuses rather than guessing -----------------------------------
run_gate "release-candidate"
check "unrecognised version refuses" 1 "refusing to release rather than guessing"
run_gate ""
check "empty version refuses" 1 "usage:"

# --- A milestone with an open blocker fails, and names it ------------------------------------
printf '  #1949 Reject nonempty but behaviorally vacuous assertions\n' > "$TEST_TEMP_DIR/blockers.txt"
run_gate "v3.38.0"
check "open blocker refuses" 1 "refusing to release"
check "open blocker is named" 1 "#1949"
: > "$TEST_TEMP_DIR/blockers.txt"

# --- The deliberate open path, pinned so it stays deliberate ---------------------------------
#
# A release with no matching milestone is not blocked. This is the ONE path where the gate passes
# without having checked anything, and all three shipped defects were accidental arrivals at it.
# Pinned by name so a fourth arrival is a test change someone has to justify.
run_gate "v9.99.0"
check "no milestone at all passes, deliberately" 0 "nothing to gate on"

# --- A failed milestone listing refuses, rather than reading as absence ----------------------
#
# Defect 1. `if ! gh api ... | grep -q` treated a rate limit as "no such milestone": `set -e` is
# suspended inside an `if` condition and `!` inverts the pipeline's status. Unreachable before the
# seam existed.
FAKE_MILESTONES_FAIL=1 run_gate "v3.38.0"
check "milestone listing failure refuses" 1 "could not list milestones"
unset FAKE_MILESTONES_FAIL

# --- A failed blocker query refuses too ------------------------------------------------------
#
# The mirror of the above, on the second call. It is fail-closed only because the assignment is not
# wrapped in an `if`; nothing else enforces that, so it is pinned.
FAKE_ISSUES_FAIL=1 run_gate "v3.38.0"
check "blocker query failure refuses" 1 ""
unset FAKE_ISSUES_FAIL

# --- A milestone past the first page is still found ------------------------------------------
#
# Defect 2. The fake honours `--paginate` exactly as the real `gh` does, so dropping the flag makes
# this case go red: page 1 would hold 100 older milestones and the release's own would be invisible.
{ for i in $(seq 1 129); do printf 'v0.%d.0\n' "$i"; done; printf 'v3.38.0\n'; } \
  > "$TEST_TEMP_DIR/milestones.txt"
run_gate "v3.38.0"
check "milestone beyond page 1 is found" 0 "no open release-blocking issues"

if ! grep -q -- '--paginate' "$TEST_TEMP_DIR/gh.log"; then
  echo "FAIL  the gate never passed --paginate"
  FAILURES=$((FAILURES + 1))
else
  echo "ok    the gate passes --paginate"
fi
printf 'v3.37.0\nv3.38.0\n' > "$TEST_TEMP_DIR/milestones.txt"

# --- The annotation is the only thing the job context changes --------------------------------
#
# The decision must not depend on running inside Actions. Same inputs, same exit code, different
# rendering.
GITHUB_ACTIONS_OVERRIDE=true run_gate "release-candidate"
check "inside Actions the refusal is annotated" 1 "::error::"
GITHUB_ACTIONS_OVERRIDE="" run_gate "release-candidate"
check "outside Actions the refusal is plain" 1 "error: unrecognised"

if [ "$FAILURES" -ne 0 ]; then
  echo
  echo "$FAILURES release-blocking gate case(s) failed"
  exit 1
fi
echo
echo "release-blocking gate: all cases pass"
