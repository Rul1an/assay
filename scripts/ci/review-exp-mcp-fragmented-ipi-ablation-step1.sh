#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-CONTRACT.md"

ALLOWLIST=(
  "$DOC_PLAN"
  "$DOC_CONTRACT"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ablation Step1 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ablation Step1: $f"
    exit 1
  fi
done

echo "[review] marker checks"
for doc in "$DOC_PLAN" "$DOC_CONTRACT"; do
  test -f "$doc" || { echo "FAIL: missing $doc"; exit 1; }
done

rg -n 'wrap-only|Variant A' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || { echo "FAIL: missing wrap-only variant"; exit 1; }
rg -n 'sequence-only|Variant B' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || { echo "FAIL: missing sequence-only variant"; exit 1; }
rg -n 'combined|Variant C' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || { echo "FAIL: missing combined variant"; exit 1; }
rg -n 'canary|CANARY_' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || { echo "FAIL: missing canary-based scoring"; exit 1; }
rg -n 'No taint|no taint|no taint/label propagation|No taint/label propagation' "$DOC_PLAN" >/dev/null || { echo "FAIL: plan must state no taint claims"; exit 1; }

echo "[review] done"
