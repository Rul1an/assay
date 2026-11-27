#!/usr/bin/env bash
# The local Linux cross-target check must show what it covered, on the path a push takes.
#
# check-linux.sh separates "our code does not compile" (exit 1) from "this host could not check"
# (exit 0, with a WARN naming the unchecked crates). pre-commit prints a passing hook's output only
# when the hook is verbose. Before this test the push log said nothing but "Passed" over a run that
# could not check half its crates: the WARN that names them was written and then hidden.
#
# Every case runs the real pre-commit against the real `linux-check` definition taken from
# .pre-commit-config.yaml and the real check-linux.sh. cargo and rustup are stubs, so these cases
# prove control flow and what a person sees. They prove nothing about whether any crate compiles
# for Linux; real cross-compilation is exercised by test-check-linux.sh and by CI on Linux.

set -euo pipefail

# Fixture repositories must not inherit the caller's Git state (GIT_INDEX_FILE under a hook).
# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="${ASSAY_LINUX_CONSUMER_CONFIG:-$ROOT/.pre-commit-config.yaml}"
GATE="${ASSAY_LINUX_CONSUMER_GATE:-$ROOT/scripts/ci/check-linux.sh}"
# The gate's deadline, and the process-group rule it imports.
DEADLINE_WRAPPER="$ROOT/scripts/precommit/run-with-timeout.py"
DEADLINE_HELPER="$ROOT/scripts/precommit/run-with-timeout.sh"
PROCESS_GROUP_LIB="$ROOT/scripts/ci/lib/process_group.py"
# The deadline suite runs from here, so the required CI step that runs this test runs it too.
DEADLINE_SUITE="$ROOT/scripts/ci/test_prepush_timeout.py"
HOOK_ID="linux-check"
EXPECTED_CASES=10

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

failures=0
executed=0
fail() {
  failures=$((failures + 1))
  printf 'FAIL: %s\n' "$*"
}
ok() { printf 'ok   %s\n' "$*"; }

if ! command -v pre-commit >/dev/null 2>&1; then
  echo "FAIL: pre-commit not on PATH (CI pin is pre-commit==4.4.0)"
  exit 1
fi
if python3 -c 'import yaml' >/dev/null 2>&1; then
  PYTHON_YAML=python3
else
  PYTHON_YAML="$(sed -n '1s/^#!//p' "$(command -v pre-commit)")"
  if ! "$PYTHON_YAML" -c 'import yaml' >/dev/null 2>&1; then
    echo "FAIL: PyYAML not importable from python3 or the pre-commit interpreter"
    exit 1
  fi
fi
# The timeout case needs no timeout(1) or gtimeout(1): the gate's deadline is run-with-timeout.py.

# --- one YAML loader for both helpers: safe, and a repeated key is an error -------------------
cat >"$TMP/unique_yaml.py" <<'PY'
import sys

import yaml


class UniqueKeyLoader(yaml.SafeLoader):
    pass


def construct_mapping(loader, node, deep=False):
    seen = set()
    for key_node, _ in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if key in seen:
            raise SystemExit(f"duplicate key {key!r} in {sys.argv[1]}")
        seen.add(key)
    return yaml.SafeLoader.construct_mapping(loader, node, deep=deep)


UniqueKeyLoader.add_constructor(
    yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, construct_mapping
)
PY

# --- the hook under test, exactly as the repository defines it ---------------------------------
EXTRACT="$TMP/extract_hook.py"
cat >"$EXTRACT" <<'PY'
import sys

import yaml

from unique_yaml import UniqueKeyLoader

src, dest, hook_id = sys.argv[1:4]
with open(src, encoding="utf-8") as fh:
    config = yaml.load(fh, Loader=UniqueKeyLoader)
found = [
    hook
    for repo in config.get("repos", [])
    if repo.get("repo") == "local"
    for hook in repo.get("hooks", [])
    if hook.get("id") == hook_id
]
if len(found) != 1:
    raise SystemExit(f"want exactly one local hook {hook_id!r}, found {len(found)}")
minimal = {"repos": [{"repo": "local", "hooks": [found[0]]}]}
with open(dest, "w", encoding="utf-8") as fh:
    yaml.safe_dump(minimal, fh, sort_keys=False)
print(found[0].get("name", ""))
PY

HOOK_NAME="$("$PYTHON_YAML" "$EXTRACT" "$CONFIG" "$TMP/minimal-config.yaml" "$HOOK_ID")"

# Required CI wiring is pinned by check-prepush-consumer-ci-step.py, from the hardening step.

# --- stubs: rustup answers the target question, cargo plays one outcome per crate --------------
STUB_BIN="$TMP/stub-bin"
mkdir -p "$STUB_BIN"
cat >"$STUB_BIN/rustup" <<'SH'
#!/usr/bin/env bash
echo "rustup $*" >>"$STUB_LOG"
case "$*" in
  "target list --installed")
    [ "${STUB_TARGET_INSTALLED:-1}" = 1 ] && echo x86_64-unknown-linux-gnu
    exit 0
    ;;
  "target add x86_64-unknown-linux-gnu")
    exit 1
    ;;
esac
echo "stub rustup: unexpected arguments: $*" >&2
exit 99
SH
cat >"$STUB_BIN/cargo" <<'SH'
#!/usr/bin/env bash
echo "cargo $*" >>"$STUB_LOG"
if [ "$#" -ne 7 ] || [ "$1" != check ] || [ "$2" != -p ] || [ "$4" != --all-targets ] ||
  [ "$5" != --target ] || [ "$6" != x86_64-unknown-linux-gnu ] || [ "$7" != --message-format=short ]; then
  echo "stub cargo: unexpected arguments: $*" >&2
  exit 98
fi
crate="$3"
mode="$(awk -v c="$crate" '$1 == c {print $2}' "$STUB_MODES" 2>/dev/null)"
case "${mode:-ok}" in
  ok) exit 0 ;;
  buildscript)
    echo "error: failed to run custom build command for \`ring v0.17.14\`"
    exit 101
    ;;
  ours)
    echo "crates/$crate/src/lib.rs:2:13: error[E0425]: cannot find value \`MISSING\` in this scope"
    exit 101
    ;;
  hang)
    exec sleep 5
    ;;
esac
echo "stub cargo: unknown mode $mode" >&2
exit 97
SH
chmod +x "$STUB_BIN/rustup" "$STUB_BIN/cargo"
for tool in cargo rustup; do
  resolved="$(env PATH="$STUB_BIN:$PATH" bash -c "command -v $tool")"
  if [ "$resolved" != "$STUB_BIN/$tool" ]; then
    echo "FAIL: $tool resolves to $resolved, not the stub; refusing to run a real build"
    exit 1
  fi
done

# Two crates carry Linux-gated code; check-linux.sh adds assay-runner-linux by name, so three.
TOTAL=3

make_fixture() {
  local dest="$1"
  mkdir -p "$dest/scripts/ci/lib" "$dest/scripts/precommit" "$dest/crates/assay-alpha/src" \
    "$dest/crates/assay-beta/src"
  cp "$GATE" "$dest/scripts/ci/check-linux.sh"
  cp "$DEADLINE_WRAPPER" "$dest/scripts/precommit/run-with-timeout.py"
  cp "$DEADLINE_HELPER" "$dest/scripts/precommit/run-with-timeout.sh"
  cp "$PROCESS_GROUP_LIB" "$dest/scripts/ci/lib/process_group.py"
  cp "$TMP/minimal-config.yaml" "$dest/.pre-commit-config.yaml"
  printf '#[cfg(target_os = "linux")]\nfn alpha() {}\n' >"$dest/crates/assay-alpha/src/lib.rs"
  printf '#[cfg(target_os = "linux")]\nfn beta() {}\n' >"$dest/crates/assay-beta/src/lib.rs"
  git -C "$dest" init -q
  git -C "$dest" add -A
}

# run_case NAME WANT_EXIT MODES [ENV=VALUE...]
run_case() {
  local name="$1" want="$2" modes="$3"
  shift 3
  local dest="$TMP/case-$name"
  make_fixture "$dest"
  printf '%s\n' "$modes" | tr ',' '\n' | tr ':' ' ' >"$dest/stub-modes"
  : >"$dest/stub.log"
  set +e
  (
    cd "$dest"
    # The gate's own defaults first, then the case's overrides (env takes the last assignment):
    # a caller who exports REQUIRE_FULL or a short timeout must not change what a case proves.
    env PATH="$STUB_BIN:$PATH" PRE_COMMIT_HOME="$TMP/pc-home" STUB_LOG="$dest/stub.log" \
      STUB_MODES="$dest/stub-modes" STUB_TARGET_INSTALLED=1 \
      ASSAY_LINUX_CHECK_REQUIRE_FULL=0 ASSAY_LINUX_TARGET_TIMEOUT=300 "$@" \
      pre-commit run "$HOOK_ID" --hook-stage pre-push --color never --all-files
  ) >"$dest/out" 2>&1
  local rc=$?
  set -e
  executed=$((executed + 1))
  LAST_OUT="$dest/out"
  LAST_LOG="$dest/stub.log"
  if [ "$rc" != "$want" ]; then
    fail "$name: pre-commit exited $rc, wanted $want"
    sed 's/^/      /' "$dest/out"
    return 1
  fi
  return 0
}

shows() {
  local name="$1" needle="$2"
  if grep -qF -- "$needle" "$LAST_OUT"; then
    return 0
  fi
  fail "$name: the push output does not show '$needle'"
  sed 's/^/      /' "$LAST_OUT"
  return 1
}

cargo_calls() { grep -c '^cargo ' "$LAST_LOG" || true; }

summary() { printf 'Linux cross-target: %s ok, %s broken, %s unchecked, of %s crate(s)' "$@"; }

# --- the name says what a pass means --------------------------------------------------------------
executed=$((executed + 1))
if printf '%s' "$HOOK_NAME" | grep -qi 'partial'; then
  ok "the hook name says a pass may be partial ($HOOK_NAME)"
else
  fail "the hook name '$HOOK_NAME' does not say that a pass may cover only part of the crates"
fi

# --- 1. target missing: nothing was checked, and the push says so --------------------------------
if run_case missing-target 0 "" STUB_TARGET_INSTALLED=0 &&
  shows missing-target "Passed" &&
  shows missing-target "Nothing was checked" &&
  shows missing-target "rustup target add x86_64-unknown-linux-gnu"; then
  if [ "$(cargo_calls)" != 0 ]; then
    fail "missing target: cargo ran $(cargo_calls) time(s) although the target is absent"
  elif grep -q '^rustup target add' "$LAST_LOG"; then
    fail "missing target: the push tried to install the target; that is setup, not a push"
  else
    ok "missing target: Passed, nothing checked, no install attempted, setup command shown (stub)"
  fi
fi

# --- 2. complete: every crate checked, and the push shows the count ------------------------------
if run_case complete 0 "" &&
  shows complete "Passed" &&
  shows complete "$(summary $TOTAL 0 0 $TOTAL)"; then
  if [ "$(cargo_calls)" = "$TOTAL" ]; then
    ok "complete: Passed with the full count visible (stub; control flow only)"
  else
    fail "complete: expected $TOTAL stub cargo runs, saw $(cargo_calls)"
  fi
fi

# --- 3. partial: a third-party build script stops one crate --------------------------------------
if run_case partial 0 "assay-beta:buildscript" &&
  shows partial "Passed" &&
  shows partial "SKIP assay-beta" &&
  shows partial "$(summary 2 0 1 $TOTAL)" &&
  shows partial "WARN: not checked, so this run says nothing about them: assay-beta"; then
  ok "partial: Passed, and the unchecked crate is named in the push output (stub; control flow only)"
fi

# --- 4. timeout: a crate that does not finish is unchecked, not passed ---------------------------
if run_case timeout 0 "assay-beta:hang" ASSAY_LINUX_TARGET_TIMEOUT=1 &&
  shows timeout "Passed" &&
  shows timeout "SKIP assay-beta -- could not be checked: timed out after 1s" &&
  shows timeout "$(summary 2 0 1 $TOTAL)"; then
  ok "timeout: the timed-out crate is named as unchecked in the push output (stub; control flow only)"
fi

# --- 5. our own compile error still fails the push ----------------------------------------------
if run_case compiler-error 1 "assay-alpha:ours" &&
  shows compiler-error "Failed" &&
  shows compiler-error "FAIL assay-alpha" &&
  shows compiler-error "$(summary 2 1 0 $TOTAL)"; then
  ok "compiler error: exit 1 and the crate is named (stub; control flow only)"
fi

# --- 6 and 7. strict mode keeps refusing incomplete coverage ------------------------------------
if run_case strict-partial 1 "assay-beta:buildscript" ASSAY_LINUX_CHECK_REQUIRE_FULL=1 &&
  shows strict-partial "Failed" &&
  shows strict-partial "coverage is incomplete"; then
  ok "REQUIRE_FULL=1 still refuses a partial run (stub; control flow only)"
fi
if run_case strict-missing-target 1 "" STUB_TARGET_INSTALLED=0 ASSAY_LINUX_CHECK_REQUIRE_FULL=1 &&
  shows strict-missing-target "Failed" &&
  shows strict-missing-target "Nothing was checked"; then
  ok "REQUIRE_FULL=1 still refuses a run that checked nothing (stub; control flow only)"
fi

# --- 8. the commit stage (and CI's `pre-commit run --all-files`) does not run it ----------------
dest="$TMP/case-default-stage"
make_fixture "$dest"
: >"$dest/stub.log"
set +e
(cd "$dest" && env PATH="$STUB_BIN:$PATH" PRE_COMMIT_HOME="$TMP/pc-home" STUB_LOG="$dest/stub.log" \
  STUB_MODES=/dev/null pre-commit run --color never --all-files) >"$dest/out" 2>&1
rc=$?
set -e
executed=$((executed + 1))
if [ "$rc" = 0 ] && ! grep -q '^cargo ' "$dest/stub.log" 2>/dev/null; then
  ok "the commit stage does not run the cross-target check"
else
  fail "the commit stage ran the cross-target check (exit $rc)"
  sed 's/^/      /' "$dest/out"
fi

# --- 9. the deadline holds for the command's process group, with or without timeout(1) ----------
# Synthetic processes only: the wrapper, both hooks with stub cargo and rustup, no build. Passing
# needs the suite's own verdict, OK over exactly DEADLINE_SUITE_TESTS tests: a skipped or
# neutralised run, or a suite that lost tests, must not read as a pass.
DEADLINE_SUITE_TESTS=36
executed=$((executed + 1))
suite_rc=0
PYTHONDONTWRITEBYTECODE=1 python3 "$DEADLINE_SUITE" >"$TMP/deadline-suite.out" 2>&1 || suite_rc=$?
suite_ran="$(sed -n 's/^Ran \([0-9][0-9]*\) tests\{0,1\} in .*/\1/p' "$TMP/deadline-suite.out" 2>/dev/null || true)"
if [ "$suite_rc" = 0 ] && [ "$suite_ran" = "$DEADLINE_SUITE_TESTS" ] &&
  grep -qx 'OK' "$TMP/deadline-suite.out" 2>/dev/null; then
  ok "deadline suite: OK over $suite_ran tests"
else
  fail "the deadline suite did not pass over $DEADLINE_SUITE_TESTS tests (exit $suite_rc, ran '${suite_ran}')"
  sed 's/^/      /' "$TMP/deadline-suite.out" 2>/dev/null
fi

if [ "$executed" -ne "$EXPECTED_CASES" ]; then
  fail "executed $executed of $EXPECTED_CASES cases"
fi
echo "executed $executed of $EXPECTED_CASES cases"
if [ "$failures" -ne 0 ]; then
  echo "$failures linux-check consumer case(s) failed"
  exit 1
fi
echo "linux-check consumer: all cases pass"
