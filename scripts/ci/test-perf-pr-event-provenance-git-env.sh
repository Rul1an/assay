#!/usr/bin/env bash
# Prove the provenance contract cannot mutate the repository that invokes it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

CALLER="$TMP/caller"
mkdir -p "$CALLER"
CALLER="$(cd "$CALLER" && pwd -P)"
git -C "$CALLER" init -q
git -C "$CALLER" config user.email "ci@example.com"
git -C "$CALLER" config user.name "CI"
printf 'caller\n' >"$CALLER/README"
git -C "$CALLER" add README
git -C "$CALLER" commit -q -m "caller"

set +e
env \
  GIT_DIR="$CALLER/.git" \
  bash "$ROOT/scripts/ci/test-perf-pr-event-provenance-contract.sh" --no-mutations \
  >/dev/null
contract_rc=$?
set -e

[[ "$(git config --file "$CALLER/.git/config" --get core.bare)" == "false" ]] || {
  echo "FAIL: provenance contract changed the caller repository to bare" >&2
  exit 1
}
[[ "$contract_rc" -eq 0 ]] || {
  echo "FAIL: provenance contract failed under inherited Git repository state" >&2
  exit 1
}
[[ "$(git -C "$CALLER" rev-parse --show-toplevel)" == "$CALLER" ]] || {
  echo "FAIL: caller repository lost its worktree" >&2
  exit 1
}

echo "ok: perf_pr provenance contract isolates inherited Git repository state"
