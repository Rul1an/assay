#!/usr/bin/env bash
# Type-check the #3176 downstream witness against this tree's assay-core.
#
# The witness depends on jsonschema 0.55.1 directly and passes and receives jsonschema::Validator
# through assay-core's public API (policy_engine::evaluate_schema and
# McpPolicy::try_compile_all_schemas). If assay-core's jsonschema moves to an incompatible 0.x
# minor, the witness stops type-checking with E0308: the downstream break this guards.
#
# The checked-in fixture is never modified. Each run copies it into a run-owned directory, resolves
# a lockfile there, and keeps the resolution, duplicate report and check log on success and on
# failure. Set ASSAY_DOWNSTREAM_WITNESS_OUT to an empty or absent directory to choose where; the
# default is a fresh mktemp directory. CARGO_TARGET_DIR is honoured.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fixture="$ROOT/tests/downstream/jsonschema-public-type"
fixture_path_line='assay-core = { path = "../../../crates/assay-core" }'
# The published public type this witness holds assay-core to. A change here is a compatibility
# decision (#3176), not a dependency update; moving the fixture pin alone fails below.
expected_pin='jsonschema = { version = "=0.55.1", default-features = false }'

grep -qxF -- "$expected_pin" "$fixture/Cargo.toml" || {
  echo "fixture no longer pins the published jsonschema type: expected '$expected_pin'" >&2
  exit 2
}
grep -qxF -- "$fixture_path_line" "$fixture/Cargo.toml" || {
  echo "fixture assay-core path line changed; expected '$fixture_path_line'" >&2
  exit 2
}

run_dir="${ASSAY_DOWNSTREAM_WITNESS_OUT:-}"
if [ -z "$run_dir" ]; then
  run_dir="$(mktemp -d "${TMPDIR:-/tmp}/assay-downstream-witness.XXXXXX")"
elif [ -e "$run_dir" ] && [ -n "$(ls -A "$run_dir")" ]; then
  echo "run directory is not empty: $run_dir" >&2
  exit 2
fi
mkdir -p "$run_dir/witness/src"
cp "$fixture/src/main.rs" "$run_dir/witness/src/main.rs"
while IFS= read -r line; do
  if [ "$line" = "$fixture_path_line" ]; then
    printf 'assay-core = { path = "%s" }\n' "$ROOT/crates/assay-core"
  else
    printf '%s\n' "$line"
  fi
done < "$fixture/Cargo.toml" > "$run_dir/witness/Cargo.toml"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target/downstream-jsonschema-witness}"
cd "$run_dir/witness"

# Deliberate resolution, retained, then a locked check against exactly what was resolved.
set +e
cargo generate-lockfile >"$run_dir/resolve.log" 2>&1
resolve_status=$?
check_status=99
if [ "$resolve_status" -eq 0 ]; then
  cargo tree --locked -e normal --duplicates >"$run_dir/duplicates.txt" 2>&1
  cargo check --locked >"$run_dir/check.log" 2>&1
  check_status=$?
fi
set -e

printf 'resolve_exit=%s\ncheck_exit=%s\n' "$resolve_status" "$check_status" >"$run_dir/status"
echo "downstream jsonschema witness: resolve_exit=$resolve_status check_exit=$check_status (retained in $run_dir)"
if [ "$resolve_status" -ne 0 ]; then
  tail -n 40 "$run_dir/resolve.log" >&2
  exit "$resolve_status"
fi
if [ "$check_status" -ne 0 ]; then
  tail -n 60 "$run_dir/check.log" >&2
fi
exit "$check_status"
