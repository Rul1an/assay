#!/usr/bin/env bash
# Prove fixture tests cannot mutate the linked worktree repository that invokes them.
set -euo pipefail

# The harness itself creates a linked worktree, so it must cross the same boundary first.
# shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
# shellcheck disable=SC1091
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/clear-git-repository-env.sh"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HELPER="$ROOT/scripts/ci/lib/clear-git-repository-env.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Keep the static production list honest against the Git version running the gate. The helper
# cannot query Git safely, but this harness has already crossed the repository-state boundary.
while IFS= read -r name; do
  # The single quotes deliberately defer indirect expansion to the isolated child shell.
  # shellcheck disable=SC2016
  if ! env "$name=sentinel" bash -c 'source "$1"; [[ -z ${!2+x} ]]' bash "$HELPER" "$name"; then
    echo "FAIL: fixture helper left repository-local variable $name set" >&2
    exit 1
  fi
done < <(git rev-parse --local-env-vars)

MAIN="$TMP/main"
WORKTREE="$TMP/worktree"
mkdir -p "$MAIN"
git -C "$MAIN" init -q
git -C "$MAIN" config user.email "ci@example.com"
git -C "$MAIN" config user.name "CI"
printf 'caller\n' >"$MAIN/README"
git -C "$MAIN" add README
git -C "$MAIN" commit -q -m "caller"
git -C "$MAIN" branch fixture
git -C "$MAIN" worktree add -q "$WORKTREE" fixture

GIT_ADMIN="$(git -C "$WORKTREE" rev-parse --git-dir)"
GIT_COMMON="$(git -C "$WORKTREE" rev-parse --git-common-dir)"

run_isolated() {
  local script="$1"
  shift
  env GIT_DIR="$GIT_ADMIN" bash "$ROOT/$script" "$@" >/dev/null
  [[ "$(git config --file "$GIT_COMMON/config" --get core.bare)" == "false" ]] || {
    echo "FAIL: $script changed the caller repository to bare" >&2
    exit 1
  }
  git -C "$WORKTREE" rev-parse --show-toplevel >/dev/null || {
    echo "FAIL: $script detached the caller's linked worktree" >&2
    exit 1
  }
}

run_isolated scripts/ci/test-perf-pr-event-provenance-contract.sh --no-mutations
run_isolated scripts/ci/test-check-release-surface.sh
run_isolated scripts/ci/test-check-python-artifact-truth.sh
run_isolated scripts/ci/test-check-python-wheel-smoke-contract.sh

echo "ok: fixture tests isolate inherited linked-worktree Git state"
