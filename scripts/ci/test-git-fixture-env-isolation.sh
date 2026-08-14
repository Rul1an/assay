#!/usr/bin/env bash
# Prove fixture tests cannot mutate the linked worktree repository that invokes them.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

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

echo "ok: fixture tests isolate inherited linked-worktree Git state"
