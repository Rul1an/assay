#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-mcp-tool-taxonomy-coverage-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-mcp-tool-taxonomy-coverage-closure.md"
  "scripts/ci/review-mcp-tool-taxonomy-coverage-closure.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: closure slice must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in closure slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'MCP Tool Taxonomy \+ Coverage Runbook \(v1\)' docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook title missing"
  exit 1
}
rg -n 'assay mcp wrap.*--coverage-out' docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md >/dev/null || {
  echo "FAIL: wrap coverage-out usage missing"
  exit 1
}
rg -n 'assay coverage.*--input' docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md >/dev/null || {
  echo "FAIL: coverage offline usage missing"
  exit 1
}
rg -n 'ADR-028|ADR-029' docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md >/dev/null || {
  echo "FAIL: ADR references missing"
  exit 1
}
rg -n 'No session/state inference in coverage v1 routes' docs/ops/MCP-TOOL-TAXONOMY-AND-COVERAGE-RUNBOOK.md >/dev/null || {
  echo "FAIL: bounded non-goal missing"
  exit 1
}

echo "[review] done"
