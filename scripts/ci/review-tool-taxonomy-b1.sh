#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "crates/assay-core/src/mcp/"
  "crates/assay-core/tests/tool_taxonomy_"
  "scripts/ci/fixtures/tool_taxonomy/"
  "scripts/ci/review-tool-taxonomy-b1.sh"
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
      [[ "$f" == "$p"* ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done

  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'struct ToolTaxonomy' crates/assay-core/src/mcp/tool_taxonomy.rs >/dev/null || {
  echo "FAIL: ToolTaxonomy missing"
  exit 1
}
rg -n 'ToolRuleSelector' crates/assay-core/src/mcp/tool_match.rs >/dev/null || {
  echo "FAIL: ToolRuleSelector missing"
  exit 1
}
rg -n 'tool_taxonomy_policy_match_handler_decision_event_records_classes' crates/assay-core/tests/tool_taxonomy_policy_match.rs >/dev/null || {
  echo "FAIL: handler decision-event test missing"
  exit 1
}

cargo test -p assay-core tool_taxonomy_policy_match
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -- -D warnings

echo "[review] done"
