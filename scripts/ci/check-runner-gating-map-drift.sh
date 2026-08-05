#!/usr/bin/env bash
set -euo pipefail

# The committed gating map must reproduce from classify_file.
#
# Three earlier attempts pinned this surface with a derived key -- the manifest
# fields, the set of rule strings, the set of uncovered prefixes -- and an
# adversarial review refuted each one the same way: a key coarse enough to
# absorb a legitimate addition also absorbs a silent removal. Demonstrated, not
# theorised: adding one entry to `all_gate_paths` left every assertion green,
# and a new classify_file branch reusing a neighbouring rule's wording ungated
# nothing while adding 43 uncovered files, also green.
#
# So the map is a snapshot and "nothing changed" is the invariant. It is the
# same shape as check-aee-seal-fixture-drift.sh, for the same reason: a derived
# artifact that nothing regenerates goes stale silently.

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

map="scripts/ci/assay_runner_gating_map.txt"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

[[ -f "$map" ]] || {
  echo "error: missing $map" >&2
  echo "Generate:  python3 scripts/ci/assay_runner_lane_check.py --emit-gating-map" >&2
  exit 1
}

cp "$map" "$scratch/committed"
python3 scripts/ci/assay_runner_lane_check.py --emit-gating-map >/dev/null
cp "$map" "$scratch/emitted"
cp "$scratch/committed" "$map"

if ! diff -u "$scratch/committed" "$scratch/emitted" >"$scratch/delta" 2>&1; then
  echo "error: the committed gating map does not match classify_file." >&2
  sed 's/^/  /' "$scratch/delta" >&2
  echo >&2
  echo "Regenerate:  python3 scripts/ci/assay_runner_lane_check.py --emit-gating-map" >&2
  echo "Read the diff before regenerating. A line that disappeared is a file" >&2
  echo "that stopped being gated, which is the failure this check exists for." >&2
  exit 1
fi

echo "gating map reproduces from classify_file ($(grep -cv '^#' "$map") gated files)."
