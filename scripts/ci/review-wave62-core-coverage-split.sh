#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-core/src/coverage.rs"
IMPL_DIR="crates/assay-core/src/coverage_next"

allowed_regex='^(crates/assay-core/src/coverage\.rs|crates/assay-core/src/coverage_next/(mod|types|analyzer|report|tests)\.rs|docs/contributing/SPLIT-(PLAN|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave62-core-coverage-split\.md|scripts/ci/review-wave62-core-coverage-split\.sh)$'

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
      docs/contributing/SPLIT-PLAN-wave62-core-coverage-split.md \
      docs/contributing/SPLIT-CHECKLIST-wave62-core-coverage-split.md \
      docs/contributing/SPLIT-MOVE-MAP-wave62-core-coverage-split.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave62-core-coverage-split.md \
      scripts/ci/review-wave62-core-coverage-split.sh
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
  echo "FAIL: Wave62 core coverage split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-core/src/lib.rs crates/assay-core/src/baseline crates/assay-core/src/model.rs crates/assay-core/src/policy crates/assay-cli crates/assay-mcp-server; then
  echo "FAIL: Wave62 core coverage split must not touch adjacent core/CLI/MCP surfaces"
  exit 1
fi

echo "[review] facade shape"
facade_loc="$(wc -l < "$FACADE" | tr -d ' ')"
if [ "$facade_loc" -gt 20 ]; then
  echo "FAIL: coverage facade grew too large: $facade_loc LOC"
  exit 1
fi
assert_rg '#\[path = "coverage_next/mod.rs"\]' "$FACADE" "coverage facade must route to coverage_next"
assert_rg 'pub use coverage_next::\{' "$FACADE" "coverage facade must re-export public API"
assert_rg 'CoverageAnalyzer' "$FACADE" "CoverageAnalyzer must be re-exported"
assert_rg 'CoverageReport' "$FACADE" "CoverageReport must be re-exported"
assert_rg 'TraceRecord' "$FACADE" "TraceRecord must be re-exported"
if rg -n 'pub struct CoverageAnalyzer|pub fn analyze|fn rule_id|fn is_policy_tool|fn is_tool_seen|impl CoverageReport|pub struct TraceRecord' "$FACADE" >/dev/null; then
  echo "FAIL: coverage facade still owns moved implementation"
  exit 1
fi

echo "[review] module ownership"
assert_rg '^mod analyzer;' "$IMPL_DIR/mod.rs" "analyzer module declaration missing"
assert_rg '^mod report;' "$IMPL_DIR/mod.rs" "report module declaration missing"
assert_rg '^mod types;' "$IMPL_DIR/mod.rs" "types module declaration missing"
assert_rg 'pub use analyzer::CoverageAnalyzer;' "$IMPL_DIR/mod.rs" "CoverageAnalyzer must be re-exported from mod.rs"
assert_rg 'pub use types::\{' "$IMPL_DIR/mod.rs" "coverage data types must be re-exported from mod.rs"
assert_rg 'pub struct ToolCoverage' "$IMPL_DIR/types.rs" "ToolCoverage must live in types.rs"
assert_rg 'pub struct CoverageReport' "$IMPL_DIR/types.rs" "CoverageReport must live in types.rs"
assert_rg 'pub struct TraceRecord' "$IMPL_DIR/types.rs" "TraceRecord must live in types.rs"
assert_rg 'pub struct CoverageAnalyzer' "$IMPL_DIR/analyzer.rs" "CoverageAnalyzer must live in analyzer.rs"
assert_rg 'pub fn from_policy' "$IMPL_DIR/analyzer.rs" "from_policy must live in analyzer.rs"
assert_rg 'pub fn analyze' "$IMPL_DIR/analyzer.rs" "analyze must live in analyzer.rs"
assert_rg 'fn rule_id' "$IMPL_DIR/analyzer.rs" "rule_id helper must live in analyzer.rs"
assert_rg 'fn is_policy_tool' "$IMPL_DIR/analyzer.rs" "policy tool helper must live in analyzer.rs"
assert_rg 'fn is_tool_seen' "$IMPL_DIR/analyzer.rs" "seen tool helper must live in analyzer.rs"
assert_rg 'impl CoverageReport' "$IMPL_DIR/report.rs" "CoverageReport impl must live in report.rs"
assert_rg 'pub fn to_github_annotation' "$IMPL_DIR/report.rs" "GitHub annotation formatter must live in report.rs"
assert_rg 'pub fn to_markdown' "$IMPL_DIR/report.rs" "markdown formatter must live in report.rs"

echo "[review] moved tests"
assert_rg 'fn test_full_coverage' "$IMPL_DIR/tests.rs" "full coverage test missing"
assert_rg 'fn test_partial_coverage' "$IMPL_DIR/tests.rs" "partial coverage test missing"
assert_rg 'fn test_unexpected_tools' "$IMPL_DIR/tests.rs" "unexpected tools test missing"
assert_rg 'fn test_github_annotation_format' "$IMPL_DIR/tests.rs" "GitHub annotation format test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-core
cargo test -p assay-core coverage
cargo clippy -p assay-core --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
