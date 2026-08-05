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
if find "$fixture_dir" -type l -print -quit | grep -q .; then
  echo "error: symlink in $fixture_dir; fixtures must be regular files:" >&2
  find "$fixture_dir" -type l >&2
  exit 1
fi

cp -RP "$fixture_dir" "$scratch/committed"
ASSAY_AEE_FIXTURE_ROOT="$scratch/emitted" \
  python3 scripts/experiments/aee_landlock_seal_fixture.py --emit >/dev/null

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
