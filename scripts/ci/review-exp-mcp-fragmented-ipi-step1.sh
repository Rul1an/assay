#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-POLICY-CONTRACT.md"

ALLOWLIST=(
  "$DOC_PLAN"
  "$DOC_CONTRACT"
  "scripts/ci/review-exp-mcp-fragmented-ipi-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step1 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in experiment Step1: $f"
    exit 1
  fi
done

echo "[review] required doc strings present"
for doc in "$DOC_PLAN" "$DOC_CONTRACT"; do
  test -f "$doc" || { echo "FAIL: missing $doc"; exit 1; }
done

rg -n "no taint|No taint|no taint tracking|No taint tracking" "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must explicitly state no taint tracking"
  exit 1
}
rg -n "sequence|stateful|NeverAfter|temporal constraint" "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention sequence/state-based enforcement"
  exit 1
}
rg -n "canary|CANARY_" "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must define canary-based success condition"
  exit 1
}
rg -n "web_search" "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention web_search sink tool"
  exit 1
}

echo "[review] done"
