#!/usr/bin/env bash
# ADR-045 seal fixtures and derivation-parity vectors are generated, not hand-written.
#
# The Rust producer in `crates/assay-cli/src/aee_seal.rs` derives `aeeRunBinding` and
# `aeeObservedSet` itself and its tests compare against `derivation-parity.json`. That comparison
# gates anything only while the committed vectors match what the emitter produces today; without
# this check a change to the Python derivation leaves stale vectors on disk and the Rust tests
# green, which is the drift the parity file exists to catch.
#
# The emitter writes into a scratch directory and nothing here touches the committed tree. An
# earlier version of this script ran `--emit` in place while its own header claimed otherwise, and
# destroyed uncommitted edits in the fixtures it was auditing.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

fixture_dir="scripts/experiments/fixtures/aee-landlock-seal"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# A symlink standing in for a fixture points the signing surface outside the repo, and both `cp -R`
# and `diff -r` follow it, so the comparison passes while the bytes under review are somewhere else.
# A symlinked *ancestor* leaves no symlink among the descendants, so checking the files is one
# component too narrow: the fixtures can live outside the repo while every file under the path is
# regular. Resolve the path and require it to be inside the worktree.
resolved_dir="$(cd "$fixture_dir" 2>/dev/null && pwd -P || true)"
resolved_root="$(cd "$repo_root" && pwd -P)"
if [[ -z "$resolved_dir" || "$resolved_dir" != "$resolved_root"/* ]]; then
  echo "error: $fixture_dir resolves to '$resolved_dir', outside the worktree at '$resolved_root'" >&2
  exit 1
fi

if find "$fixture_dir" -type l -print -quit | grep -q .; then
  echo "error: symlink in $fixture_dir; fixtures must be regular files:" >&2
  find "$fixture_dir" -type l >&2
  exit 1
fi

cp -RP "$fixture_dir" "$scratch/committed"

# Run a *copy* of the emitter rather than redirecting the real one. The emitter derives its output
# root from `__file__`, so a copy writes under the copy with nothing to pass it -- and the override
# that used to do this was an arbitrary-write primitive in a script this hook invokes.
cp -RP scripts/experiments "$scratch/experiments"
rm -rf "$scratch/experiments/fixtures/aee-landlock-seal"
python3 "$scratch/experiments/aee_landlock_seal_fixture.py" --emit >/dev/null
mv "$scratch/experiments/fixtures/aee-landlock-seal" "$scratch/emitted"

# Compare both directions. `--emit` only writes, so a case removed from CASES leaves its fixture on
# disk unmodified: `git diff` stays clean and the retired control looks authoritative while nothing
# tests it. Only a set comparison sees that.
if ! diff -r -q "$scratch/committed" "$scratch/emitted" >"$scratch/delta" 2>&1; then
  echo "error: ADR-045 seal fixtures do not match the emitter." >&2
  sed 's/^/  /' "$scratch/delta" >&2
  echo >&2
  echo "Regenerate:  python3 scripts/experiments/aee_landlock_seal_fixture.py --emit" >&2
  echo "A file the emitter no longer writes belongs deleted, not regenerated." >&2
  exit 1
fi

echo "ADR-045 seal fixtures reproduce from the emitter, with no extra files."
