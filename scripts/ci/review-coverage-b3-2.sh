#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-cli/src/cli/args/mod.rs"
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/coverage.rs"
  "crates/assay-cli/src/cli/commands/coverage/report.rs"
  "crates/assay-cli/tests/mcp_wrap_coverage.rs"
  "scripts/ci/fixtures/coverage/decision_event_basic.jsonl"
  "scripts/ci/fixtures/coverage/README.md"
  "scripts/ci/review-coverage-b3-2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'coverage_out' crates/assay-cli/src/cli/args/mod.rs >/dev/null || {
  echo "FAIL: McpWrapArgs missing coverage_out"
  exit 1
}
rg -n 'normalize_decision_jsonl_to_coverage_jsonl|collect_declared_tools' crates/assay-cli/src/cli/commands/mcp.rs >/dev/null || {
  echo "FAIL: mcp wrap missing coverage helpers"
  exit 1
}
rg -n 'mcp_wrap_coverage_normalizer_and_report_writer_emit_v1_report' crates/assay-cli/src/cli/commands/mcp.rs >/dev/null || {
  echo "FAIL: wrap coverage writer test missing"
  exit 1
}

cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli coverage_contract
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings

echo "[review] done"
