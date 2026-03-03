#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-SECOND-SINK-2026q1.md"
  "docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-SECOND-SINK-CONTRACT.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-second-sink-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: no workflows in second sink Step1 ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in second sink Step1: $f"
    exit 1
  fi
done

echo "[review] marker checks"
DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-SECOND-SINK-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-SECOND-SINK-CONTRACT.md"

rg -n 'web_search' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing primary sink label"
  exit 1
}
rg -n 'web_search_alt' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing alternate sink label"
  exit 1
}
rg -n 'Condition A - primary sink only' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing Condition A"
  exit 1
}
rg -n 'Condition B - alternate sink only' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing Condition B"
  exit 1
}
rg -n 'Condition C - mixed sink path' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing Condition C"
  exit 1
}
rg -n 'first decisive blocker observed' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing blocker attribution requirement"
  exit 1
}
rg -n 'no taint tracking' "$DOC_PLAN" >/dev/null || {
  echo "FAIL: plan must state no taint tracking"
  exit 1
}

echo "[review] done"
