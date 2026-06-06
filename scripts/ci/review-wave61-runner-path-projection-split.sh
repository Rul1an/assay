#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-runner-core/src/path_projection.rs"
IMPL_DIR="crates/assay-runner-core/src/path_projection_next"

allowed_regex='^(crates/assay-runner-core/src/path_projection\.rs|crates/assay-runner-core/src/path_projection_next/(mod|project|tests)\.rs|docs/contributing/SPLIT-(PLAN|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave61-runner-path-projection-split\.md|scripts/ci/review-wave61-runner-path-projection-split\.sh)$'

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] scope allowlist"
changed_files="$(
  {
    git diff --name-only "$BASE_REF" --
    git ls-files --others --exclude-standard -- \
      "$FACADE" \
      "$IMPL_DIR" \
      docs/contributing/SPLIT-PLAN-wave61-runner-path-projection-split.md \
      docs/contributing/SPLIT-CHECKLIST-wave61-runner-path-projection-split.md \
      docs/contributing/SPLIT-MOVE-MAP-wave61-runner-path-projection-split.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave61-runner-path-projection-split.md \
      scripts/ci/review-wave61-runner-path-projection-split.sh
  } | sort -u
)"
while IFS= read -r file; do
  [ -n "$file" ] || continue
  if [[ ! "$file" =~ $allowed_regex ]]; then
    echo "FAIL: out-of-scope path changed: $file"
    exit 1
  fi
done <<EOF
$changed_files
EOF

echo "[review] forbidden drift"
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave61 path projection split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-runner-core/src/archive.rs crates/assay-runner-core/src/kernel.rs crates/assay-runner-core/src/kernel crates/assay-runner-core/src/policy.rs crates/assay-runner-core/src/run.rs crates/assay-runner-core/src/sdk.rs crates/assay-runner-core/src/lib.rs; then
  echo "FAIL: Wave61 path projection split must not touch other runner-core surfaces"
  exit 1
fi

echo "[review] facade shape"
facade_loc="$(wc -l < "$FACADE" | tr -d ' ')"
if [ "$facade_loc" -gt 20 ]; then
  echo "FAIL: path_projection facade grew too large: $facade_loc LOC"
  exit 1
fi
assert_rg '#\[path = "path_projection_next/mod.rs"\]' "$FACADE" "path_projection facade must route to path_projection_next"
assert_rg 'pub use path_projection_next::\{' "$FACADE" "path_projection facade must re-export public API"
assert_rg 'project_filesystem_paths' "$FACADE" "project_filesystem_paths must be re-exported"
assert_rg 'PATH_PROJECTION_SCHEMA' "$FACADE" "PATH_PROJECTION_SCHEMA must be re-exported"
if rg -n 'fn map_|fn exact_rule_map|fn split_operation_path|fn with_operation|struct DeclaredPathProjectionRules' "$FACADE" >/dev/null; then
  echo "FAIL: path_projection facade still owns moved implementation"
  exit 1
fi

echo "[review] module ownership"
assert_rg '^mod project;' "$IMPL_DIR/mod.rs" "project module declaration missing"
assert_rg 'pub const PATH_PROJECTION_SCHEMA' "$IMPL_DIR/mod.rs" "schema constant must live in mod.rs"
assert_rg 'pub struct DeclaredPathProjectionRules' "$IMPL_DIR/mod.rs" "rules type must live in mod.rs"
assert_rg 'pub struct DeclaredPathRule' "$IMPL_DIR/mod.rs" "rule type must live in mod.rs"
assert_rg 'pub struct PathProjection' "$IMPL_DIR/mod.rs" "projection type must live in mod.rs"
assert_rg 'pub use project::project_filesystem_paths;' "$IMPL_DIR/mod.rs" "project function must be re-exported from mod.rs"
assert_rg 'pub fn project_filesystem_paths' "$IMPL_DIR/project.rs" "project function must live in project.rs"
assert_rg 'fn exact_rule_map' "$IMPL_DIR/project.rs" "exact rule helper must live in project.rs"
assert_rg 'fn map_exact' "$IMPL_DIR/project.rs" "exact mapping helper must live in project.rs"
assert_rg 'fn map_workdir_prefix' "$IMPL_DIR/project.rs" "prefix mapping helper must live in project.rs"
assert_rg 'fn split_operation_path' "$IMPL_DIR/project.rs" "operation parser must live in project.rs"

echo "[review] moved tests"
assert_rg 'fn declared_workload_paths_project_to_roles_without_rewriting_raw_set' "$IMPL_DIR/tests.rs" "declared workload path test missing"
assert_rg 'fn declared_workdir_prefix_projects_inside_paths_and_preserves_operation_prefixes' "$IMPL_DIR/tests.rs" "workdir prefix test missing"
assert_rg 'fn exact_declared_rules_win_over_declared_workdir_prefixes' "$IMPL_DIR/tests.rs" "exact precedence test missing"
assert_rg 'fn unknown_paths_are_summarized_not_failed_or_collapsed' "$IMPL_DIR/tests.rs" "unknown path test missing"
assert_rg 'fn projection_is_deterministic_for_repeated_runs' "$IMPL_DIR/tests.rs" "determinism test missing"
assert_rg 'fn different_raw_paths_can_share_a_declared_projected_role_without_equivalence_claim' "$IMPL_DIR/tests.rs" "non-equivalence test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-runner-core
cargo test -p assay-runner-core path_projection
cargo clippy -p assay-runner-core --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
