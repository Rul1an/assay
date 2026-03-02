#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-LIVE-ENABLE-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-LIVE-CONTRACT.md"

ALLOWLIST=(
  "$DOC_PLAN"
  "$DOC_CONTRACT"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-live-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ablation live StepA must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ablation live StepA: $f"
    exit 1
  fi
done

echo "[review] marker checks"
for doc in "$DOC_PLAN" "$DOC_CONTRACT"; do
  test -f "$doc" || { echo "FAIL: missing $doc"; exit 1; }
done

rg -n 'RUN_LIVE=1|RUN_LIVE=0' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must freeze live/offline mode semantics"
  exit 1
}
rg -n 'MCP_HOST_CMD|MCP_HOST_ARGS|ASSAY_CMD' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention live env contract"
  exit 1
}
rg -n 'wrap_only|sequence_only|combined|Variant A|Variant B|Variant C' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention all ablation variants"
  exit 1
}
rg -n 'ABLATION_MODE=|SIDECAR=enabled\|disabled|ASSAY_POLICY=|MCP_HOST_CMD=' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must freeze logging invariants"
  exit 1
}
rg -n 'No taint|no taint|No taint/label propagation|no taint/label propagation' "$DOC_PLAN" >/dev/null || {
  echo "FAIL: plan must state no taint claims"
  exit 1
}
rg -n 'no absolute user-specific paths hardcoded|no absolute user paths hardcoded' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must freeze no-hardcoded-user-paths constraint"
  exit 1
}

echo "[review] done"
