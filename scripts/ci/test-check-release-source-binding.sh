#!/usr/bin/env bash
# Contract for scripts/ci/check-release-source-binding.sh and its call site in release.yml.
#
# The required checks protect `main`, but publishing read none of them: release.yml accepted a tag
# push or a dispatch on any commit and published it to every registry. v7.0.2, v7.0.3 and v7.0.4
# were published from commits that are not on `main`, with no `CI`, review-record or lane-check run
# on those commits. This test pins the rule that closes that path: a release commit must be on
# `main`, or its annotated tag must say why it is not.
#
# Every case is judged by exit status. A case that expects a refusal fails when the check passes,
# and the reverse.
set -euo pipefail

# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="${ROOT}/scripts/ci/check-release-source-binding.sh"
RELEASE_WF="${ROOT}/.github/workflows/release.yml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

failures=0
pass() { printf 'PASS: %s\n' "$*"; }
fail() { printf 'FAIL: %s\n' "$*" >&2; failures=$((failures + 1)); }

if [[ ! -f "$CHECK" ]]; then
  printf 'FAIL: %s is missing\n' "${CHECK#"${ROOT}"/}" >&2
  exit 1
fi

# --- Fixture: a bare origin with main, plus one commit that never reached main. ---------------
git_q() { git -c init.defaultBranch=main -c user.name=fixture -c user.email=fixture@example.invalid \
  -c commit.gpgsign=false -c tag.gpgsign=false "$@"; }

git_q init -q --bare "$TMP/origin.git"
git_q clone -q "$TMP/origin.git" "$TMP/author" 2>/dev/null
(
  cd "$TMP/author"
  echo one > file && git_q add file && git_q commit -q -m "main one"
  echo two > file && git_q add file && git_q commit -q -m "main two"
  git_q push -q origin main
  git_q checkout -q -b side
  echo side > file && git_q add file && git_q commit -q -m "side only"
  side_sha="$(git rev-parse HEAD)"
  main_sha="$(git rev-parse main)"
  git_q tag -a v1.0.0 -m "Assay v1.0.0" "$main_sha"
  git_q tag -a v1.0.1 -m "Assay v1.0.1" "$side_sha"
  git_q tag -a v1.0.2 -m "Assay v1.0.2

Release-Source-Exception: embargoed fix published before the advisory, merged to main afterwards" "$side_sha"
  git_q tag v1.0.3 "$side_sha"
  git_q tag -a v1.0.4 -m "Assay v1.0.4

Release-Source-Exception: too short" "$side_sha"
  git_q tag -a v1.0.5 -m "Assay v1.0.5

Release-Source-Exception: embargoed fix published before the advisory, merged to main afterwards" "$main_sha"
  git_q tag -a v1.0.6 -m "Assay v1.0.6

Release-Source-Exception: embargoed fix published before the advisory, merged to main afterwards
Release-Source-Exception: a second declaration that makes the first one ambiguous to a reader" "$side_sha"
  git_q push -q origin side --tags
) >/dev/null

MAIN_SHA="$(git -C "$TMP/author" rev-parse main)"
OLD_MAIN_SHA="$(git -C "$TMP/author" rev-parse main~1)"
SIDE_SHA="$(git -C "$TMP/author" rev-parse side)"

# The release checkout: a full clone of origin, as release.yml's release-contract job makes one.
git_q clone -q "$TMP/origin.git" "$TMP/release" 2>/dev/null
# A shallow clone, which cannot prove ancestry.
git_q clone -q --depth 1 --no-local "$TMP/origin.git" "$TMP/shallow" 2>/dev/null

run_check() {
  # run_check <checkout> <sha> <tag> [extra env...]
  local checkout="$1" sha="$2" tag="$3"
  shift 3
  (cd "$checkout" && env RELEASE_SHA="$sha" RELEASE_TAG="$tag" "$@" bash "$CHECK") \
    >"$TMP/out" 2>&1
}

expect_pass() {
  local name="$1"
  shift
  if run_check "$@"; then
    pass "$name"
  else
    fail "$name: expected the release source to be accepted"
    sed 's/^/  | /' "$TMP/out" >&2
  fi
}

expect_refuse() {
  local name="$1" needle="$2"
  shift 2
  if run_check "$@"; then
    fail "$name: expected a refusal, the check passed"
    sed 's/^/  | /' "$TMP/out" >&2
  elif ! grep -Fq -- "$needle" "$TMP/out"; then
    fail "$name: refused, but without naming the reason '$needle'"
    sed 's/^/  | /' "$TMP/out" >&2
  else
    pass "$name"
  fi
}

# --- Behaviour ------------------------------------------------------------------------------------
expect_pass "main tip is accepted" "$TMP/release" "$MAIN_SHA" v1.0.0
expect_pass "an older main commit is accepted" "$TMP/release" "$OLD_MAIN_SHA" v1.0.0
expect_refuse "off-main commit without a tag is refused" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v9.9.9
expect_refuse "off-main commit with a plain annotated tag is refused" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.1
expect_pass "off-main commit with a declared exception is accepted" \
  "$TMP/release" "$SIDE_SHA" v1.0.2
if grep -Fq "embargoed fix published before the advisory" "$TMP/out" \
  && grep -Fq "::warning" "$TMP/out"; then
  pass "an accepted exception is announced with its reason"
else
  fail "an accepted exception must be announced as a warning that quotes its reason"
  sed 's/^/  | /' "$TMP/out" >&2
fi
expect_refuse "a lightweight tag cannot carry an exception" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.3
expect_refuse "an exception reason below the floor is refused" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.4
expect_refuse "an exception on a tag that names another commit is refused" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.5
expect_refuse "two exception lines are refused" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.6
expect_refuse "a shallow checkout cannot prove ancestry" "shallow" \
  "$TMP/shallow" "$MAIN_SHA" v1.0.0
expect_refuse "a missing main ref is refused, not treated as clean" "cannot read" \
  "$TMP/release" "$MAIN_SHA" v1.0.0 RELEASE_MAIN_REF=refs/remotes/origin/absent
expect_refuse "a malformed SHA is refused" "40 lowercase hex" \
  "$TMP/release" "not-a-sha" v1.0.0
expect_refuse "a malformed tag is refused" "release tag must be" \
  "$TMP/release" "$MAIN_SHA" "v1.0.0;rm"
expect_refuse "an unreachable remote is refused, not treated as clean" "is not on main" \
  "$TMP/release" "$SIDE_SHA" v1.0.2 RELEASE_REMOTE="$TMP/no-such-remote.git"

# --- Call site ------------------------------------------------------------------------------------
# The check only protects a release if release.yml runs it, in the job every publishing job
# depends on, on a checkout deep enough to answer the question.
if python3 - "$RELEASE_WF" <<'PY'
import re
import sys

text = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"(?ms)^  release-contract:\n(.*?)(?=^  [a-zA-Z][\w-]*:\n|\Z)", text)
if not match:
    sys.exit("release.yml has no release-contract job")
job = match.group(1)
active = [line for line in job.splitlines() if not line.lstrip().startswith("#")]
body = "\n".join(active)
problems = []
if not re.search(r"(?m)^          fetch-depth: 0$", body):
    problems.append("release-contract checkout must set fetch-depth: 0")
if not re.search(r"(?m)^        run: bash scripts/ci/check-release-source-binding\.sh$", body):
    problems.append("release-contract must run scripts/ci/check-release-source-binding.sh")
for line in (
    "RELEASE_SHA: ${{ github.sha }}",
    "RELEASE_TAG: ${{ steps.version.outputs.version }}",
):
    if line not in body:
        problems.append(f"binding step environment missing {line!r}")
step = re.search(
    r"(?ms)^      - name: Bind release source to main\n(.*?)(?=^      - |\Z)", body
)
if step is None:
    problems.append("release-contract has no 'Bind release source to main' step")
else:
    for forbidden in ("if:", "continue-on-error:"):
        if re.search(rf"(?m)^        {re.escape(forbidden)}", step.group(1)):
            problems.append(f"the binding step must not use {forbidden}")
if problems:
    sys.exit("\n".join(problems))
PY
then
  pass "release.yml runs the binding check in release-contract on a full checkout"
else
  fail "release.yml does not run the binding check as release-contract requires"
fi

if [[ "$failures" -ne 0 ]]; then
  printf '%d release source binding case(s) failed\n' "$failures" >&2
  exit 1
fi
echo "release source binding contract: all cases passed"
