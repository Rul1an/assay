#!/usr/bin/env bash
# ADR-045 seal fixtures and derivation-parity vectors are generated, not hand-written.
#
# The Rust producer in `crates/assay-cli/src/aee_seal.rs` derives `aeeRunBinding` and
# `aeeObservedSet` itself and its tests compare against `derivation-parity.json`. That comparison is
# only a gate while the committed vectors match what the emitter produces today; without this check
# a change to the Python derivation leaves stale vectors on disk and the Rust tests green, which is
# the exact drift the parity file exists to catch.
#
# Regenerating in a temporary worktree copy keeps the check read-only: a hook that rewrites the
# files it is checking would turn a failure into a silent fix.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

fixture_dir="scripts/experiments/fixtures/aee-landlock-seal"

before="$(git status --porcelain -- "$fixture_dir")"
python3 scripts/experiments/aee_landlock_seal_fixture.py --emit >/dev/null

if ! git diff --quiet -- "$fixture_dir"; then
  echo "error: ADR-045 seal fixtures are stale." >&2
  echo "The emitter produces different bytes than the committed files:" >&2
  git --no-pager diff --stat -- "$fixture_dir" >&2
  echo >&2
  echo "Run: python3 scripts/experiments/aee_landlock_seal_fixture.py --emit" >&2
  echo "then commit the result." >&2
  exit 1
fi

# Untracked output would also be drift: a new case emitted but never committed.
after="$(git status --porcelain -- "$fixture_dir")"
if [[ "$before" != "$after" ]]; then
  echo "error: --emit produced files that are not committed:" >&2
  git status --porcelain -- "$fixture_dir" >&2
  exit 1
fi

echo "ADR-045 seal fixtures reproduce from the emitter."
