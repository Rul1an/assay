#!/usr/bin/env bash
# Local pre-push clippy must skip exactly when CI's lightweight classifier would.
#
# scripts/precommit/cargo-clippy.sh skips the workspace lint for a "lightweight" diff and says it
# mirrors CI. It kept its own regex (all of docs/ and every Markdown file) after CI narrowed the
# rule: Rust reads active docs and crate Markdown through include_str!, so CI runs the full lane
# for them. It also read the diff with git's rename detection, which reports only the destination,
# so a crate Markdown file moved into docs/archive/ looked lightweight. CI reads both sides with
# --no-renames.
#
# Each case commits one change in a fixture repository and runs the real hook with a stub cargo.
# The verdict is checked twice: against CI's classifier on the --no-renames list, and against the
# table below, so the cases are known to exercise both outcomes. A stub cargo proves only whether
# clippy would have been started, never that the workspace lints.

set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HOOK="${ASSAY_CLIPPY_SCOPE_HOOK:-$ROOT/scripts/precommit/cargo-clippy.sh}"
CLASSIFIER="$ROOT/scripts/ci/classify-lightweight-changes.sh"
EXPECTED_CASES=13

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

failures=0
executed=0
fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}
ok() { printf 'ok   %s\n' "$*"; }

# The hook puts ${CARGO_HOME:-$HOME/.cargo}/bin first on PATH, so the stub must live there; a stub
# that is only first on the caller's PATH would lose to the real cargo and start a real lint.
CARGO_HOME_STUB="$TMP/cargo-home"
STUB_BIN="$CARGO_HOME_STUB/bin"
mkdir -p "$STUB_BIN"
cat >"$STUB_BIN/cargo" <<'SH'
#!/usr/bin/env bash
echo "cargo $*" >>"$STUB_LOG"
exit 0
SH
chmod +x "$STUB_BIN/cargo"
# The single quotes defer expansion to the child shell, which mirrors the hook's PATH line.
# shellcheck disable=SC2016
resolved="$(env CARGO_HOME="$CARGO_HOME_STUB" bash -c 'export PATH="${CARGO_HOME}/bin:$PATH"; command -v cargo')"
if [ "$resolved" != "$STUB_BIN/cargo" ]; then
  echo "FAIL: cargo resolves to $resolved, not the stub; refusing to run a real lint"
  exit 1
fi

# A fixture Git that ignores the caller's global and system configuration, with rename detection
# forced on: that is git's default, and the case the old hook got wrong.
fgit() {
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git -C "$REPO" \
    -c user.name=fixture -c user.email=fixture@example.invalid -c diff.renames=true \
    -c core.hooksPath=/dev/null -c commit.gpgsign=false "$@"
}

REPO="$TMP/repo"
mkdir -p "$REPO/scripts/precommit" "$REPO/scripts/ci/lib" "$REPO/docs/guides" "$REPO/docs/archive" \
  "$REPO/crates/assay-core" "$REPO/crates/assay-cli/src/exit_codes"
cp "$HOOK" "$REPO/scripts/precommit/cargo-clippy.sh"
cp "$CLASSIFIER" "$REPO/scripts/ci/classify-lightweight-changes.sh"
cp "$ROOT/scripts/ci/lib/pushed-range-changes.sh" "$REPO/scripts/ci/lib/pushed-range-changes.sh"
# These cases exercise the manual-base path, not the push that invokes this test.
unset PRE_COMMIT_FROM_REF PRE_COMMIT_TO_REF
body() { printf '%s\n' "# $1" "" "line one of $1" "line two of $1" "line three of $1" "line four"; }
body "active guide" >"$REPO/docs/guides/a.md"
body "archived history" >"$REPO/docs/archive/h.md"
body "crate readme" >"$REPO/crates/assay-core/README.md"
body "exit code boundary" >"$REPO/crates/assay-cli/src/exit_codes/b.md"
body "helper" >"$REPO/scripts/ci/helper.py"
body "site" >"$REPO/mkdocs.yml"
fgit init -q
fgit add -A
fgit commit -q -m base
BASE="$(fgit rev-parse HEAD)"

# run_case NAME WANT(run|skip) CHANGE...
run_case() {
  local name="$1" want="$2"
  shift 2
  fgit checkout -q -B "case-$name" "$BASE"
  if [ "$#" -gt 0 ]; then
    "$@"
    fgit add -A
    fgit commit -q -m "$name"
  fi
  # A rename case proves nothing unless git itself records the change as a rename.
  case "$name" in
    rename-*)
      if ! fgit diff -M --name-status "$BASE...HEAD" | grep -qE '^R[0-9]+'; then
        executed=$((executed + 1))
        fail "$name: git did not record a rename, so the case would prove nothing"
        return
      fi
      ;;
  esac
  local log="$TMP/$name.cargo.log" out="$TMP/$name.out"
  : >"$log"
  set +e
  (
    cd "$REPO"
    env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 CARGO_HOME="$CARGO_HOME_STUB" \
      PATH="$STUB_BIN:$PATH" STUB_LOG="$log" ASSAY_PREPUSH_BASE_REF="$BASE" CLIPPY_TIMEOUT=0 \
      CARGO_TARGET_DIR="$TMP/target" bash scripts/precommit/cargo-clippy.sh
  ) >"$out" 2>&1
  local rc=$?
  set -e
  executed=$((executed + 1))
  if [ "$rc" -ne 0 ]; then
    fail "$name: hook exited $rc"
    sed 's/^/      /' "$out"
    return
  fi
  local actual=skip
  grep -q '^cargo clippy ' "$log" && actual=run

  fgit diff --no-renames --name-only "$BASE...HEAD" >"$TMP/$name.list"
  local ci=run
  [ "$(bash "$CLASSIFIER" "$TMP/$name.list")" = true ] && ci=skip

  if [ "$actual" != "$ci" ]; then
    fail "$name: local clippy would $actual, CI's classifier says $ci (paths: $(tr '\n' ' ' <"$TMP/$name.list"))"
    sed 's/^/      /' "$out"
  elif [ "$actual" != "$want" ]; then
    fail "$name: local clippy and CI agree on $actual, but this case expects $want"
  else
    ok "$name: clippy would $actual, as CI's classifier says (stub cargo)"
  fi
}

append() { printf 'edited\n' >>"$REPO/$1"; }
move() { fgit mv "$1" "$2"; }

run_case active-doc run append docs/guides/a.md
run_case crate-markdown run append crates/assay-core/README.md
run_case python-script run append scripts/ci/helper.py
run_case archive-doc skip append docs/archive/h.md
run_case mkdocs skip append mkdocs.yml
# Rename sides: only the source is heavy, only the destination is heavy, neither is.
run_case rename-crate-markdown-into-archive run move crates/assay-cli/src/exit_codes/b.md docs/archive/b.md
run_case rename-archive-into-active-docs run move docs/archive/h.md docs/guides/h.md
run_case rename-within-archive skip move docs/archive/h.md docs/archive/h-old.md
# No change since the base is not lightweight for CI, so clippy runs.
run_case no-change run

# --- negative controls: nothing but a clean "true" from a classifier that succeeded skips ------
#
# Each control commits a change the healthy classifier calls lightweight (the archive-doc case
# above skips), then commits the broken classifier in this disposable fixture. Keeping it clean
# reaches the classifier instead of the dirty-worktree fallback. A broken classifier must start
# clippy. A diff that cannot be listed keeps its existing outcome: the hook stops non-zero, and
# never reports a lightweight skip.
# run_control NAME WANT(run|stop) SABOTAGE [ENV=VALUE...]
run_control() {
  local name="$1" want="$2" sabotage="$3"
  shift 3
  fgit checkout -q -f -B "control-$name" "$BASE"
  fgit clean -q -fd
  append docs/archive/h.md
  fgit add -A
  fgit commit -q -m "$name"
  "$sabotage"
  if ! fgit diff --quiet; then
    fgit add -A
    fgit commit -q -m "broken-classifier-$name"
  fi
  local log="$TMP/$name.cargo.log" out="$TMP/$name.out"
  : >"$log"
  set +e
  (
    cd "$REPO"
    env GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 CARGO_HOME="$CARGO_HOME_STUB" \
      PATH="$STUB_BIN:$PATH" STUB_LOG="$log" ASSAY_PREPUSH_BASE_REF="$BASE" CLIPPY_TIMEOUT=0 \
      CARGO_TARGET_DIR="$TMP/target" "$@" bash scripts/precommit/cargo-clippy.sh
  ) >"$out" 2>&1
  local rc=$?
  set -e
  executed=$((executed + 1))
  if grep -q 'lightweight-only diff detected' "$out"; then
    fail "$name: the hook reported a lightweight skip"
    sed 's/^/      /' "$out"
  elif [ "$want" = stop ]; then
    if [ "$rc" -ne 0 ] && ! grep -q '^cargo ' "$log"; then
      ok "$name: the hook stops with exit $rc and starts nothing (stub cargo)"
    else
      fail "$name: wanted a non-zero stop without cargo, got exit $rc"
      sed 's/^/      /' "$out"
    fi
  elif [ "$rc" -ne 0 ]; then
    fail "$name: hook exited $rc instead of running clippy"
    sed 's/^/      /' "$out"
  elif ! grep -q '^cargo clippy ' "$log"; then
    fail "$name: clippy was skipped"
    sed 's/^/      /' "$out"
  else
    ok "$name: clippy runs (stub cargo)"
  fi
  fgit checkout -q -f "control-$name"
  fgit clean -q -fd
}

CLASSIFIER_PATH="scripts/ci/classify-lightweight-changes.sh"
no_classifier() { rm -f "$REPO/$CLASSIFIER_PATH"; }
classifier_true_but_fails() { printf '#!/usr/bin/env bash\necho true\nexit 1\n' >"$REPO/$CLASSIFIER_PATH"; }
classifier_unknown_answer() { printf '#!/usr/bin/env bash\necho yes\n' >"$REPO/$CLASSIFIER_PATH"; }
no_sabotage() { :; }

run_control classifier-missing run no_classifier
run_control classifier-says-true-but-fails run classifier_true_but_fails
run_control classifier-unknown-answer run classifier_unknown_answer
run_control diff-fails stop no_sabotage ASSAY_PREPUSH_BASE_REF=refs/heads/no-such-base

if [ "$executed" -ne "$EXPECTED_CASES" ]; then
  fail "executed $executed of $EXPECTED_CASES cases"
fi
echo "executed $executed of $EXPECTED_CASES cases"
if [ "$failures" -ne 0 ]; then
  echo "$failures clippy scope case(s) failed"
  exit 1
fi
echo "cargo-clippy lightweight scope: all cases pass"
