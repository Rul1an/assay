#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/src/cli/commands/coverage.rs"
  "crates/assay-cli/src/cli/commands/coverage/report.rs"
  "crates/assay-cli/src/cli/commands/coverage/format_md.rs"
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/session_state_window.rs"
  "crates/assay-cli/tests/coverage_format_md.rs"
  "crates/assay-cli/tests/coverage_declared_tools_file.rs"
  "scripts/ci/fixtures/coverage/declared_tools_basic.txt"
  "scripts/ci/review-b4b-dx-impl.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: B4B must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in B4B: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'declared_tools_file|declared-tools-file' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: missing --declared-tools-file flag in args"
  exit 1
}
rg -n 'format.*md|--format.*md' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: missing --format md wiring in args"
  exit 1
}
test -f crates/assay-cli/src/cli/commands/coverage/format_md.rs || {
  echo "FAIL: missing format_md.rs implementation"
  exit 1
}
rg -n 'render_coverage_markdown|format_md' crates/assay-cli/src/cli/commands/coverage/format_md.rs >/dev/null || {
  echo "FAIL: format_md.rs missing render function markers"
  exit 1
}

# Keep exit priority invariant: wrapped > coverage > state-window
rg -n 'wrapped.*exit|wrapped_code' crates/assay-cli/src/cli/commands/mcp.rs >/dev/null || {
  echo "FAIL: mcp wrap exit priority markers missing"
  exit 1
}

# Tests present
rg -n 'coverage_format_md' crates/assay-cli/tests/coverage_format_md.rs >/dev/null || {
  echo "FAIL: missing coverage_format_md test"
  exit 1
}
rg -n 'coverage_declared_tools_file' crates/assay-cli/tests/coverage_declared_tools_file.rs >/dev/null || {
  echo "FAIL: missing declared-tools-file test"
  exit 1
}

echo "[review] run targeted tests + fmt + clippy"
cargo test -p assay-cli coverage_format_md
cargo test -p assay-cli coverage_declared_tools_file
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings

echo "[review] done"
