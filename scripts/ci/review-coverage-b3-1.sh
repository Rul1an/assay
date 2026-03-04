#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "crates/assay-cli/src/cli/commands/coverage.rs"
  "crates/assay-cli/src/cli/commands/coverage/"
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/tests/coverage_contract.rs"
  "scripts/ci/fixtures/coverage/"
  "scripts/ci/review-coverage-b3-1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }

  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done

  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
test -f crates/assay-cli/src/cli/commands/coverage/report.rs || {
  echo "FAIL: coverage report builder missing"
  exit 1
}
test -f crates/assay-cli/src/cli/commands/coverage/schema.rs || {
  echo "FAIL: coverage schema validator missing"
  exit 1
}
test -f scripts/ci/fixtures/coverage/input_tool_name_fallback.jsonl || {
  echo "FAIL: fallback fixture missing"
  exit 1
}
test -f scripts/ci/fixtures/coverage/input_missing_tool_fields.jsonl || {
  echo "FAIL: negative fixture missing"
  exit 1
}
rg -n "missing required field: 'tool' or 'tool_name'" crates/assay-cli/tests/coverage_contract.rs >/dev/null || {
  echo "FAIL: negative coverage contract test missing"
  exit 1
}

cargo test -p assay-cli coverage_contract
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings

echo "[review] done"
