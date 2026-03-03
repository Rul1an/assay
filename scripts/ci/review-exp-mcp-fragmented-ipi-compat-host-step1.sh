#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-COMPAT-HOST-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-COMPAT-HOST-CONTRACT.md"

ALLOWLIST=(
  "$DOC_PLAN"
  "$DOC_CONTRACT"
  "scripts/ci/review-exp-mcp-fragmented-ipi-compat-host-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: compat-host Step1 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in compat-host Step1: $f"
    exit 1
  fi
done

echo "[review] marker checks"
for doc in "$DOC_PLAN" "$DOC_CONTRACT"; do
  test -f "$doc" || { echo "FAIL: missing $doc"; exit 1; }
done

rg -n 'experiment-only|experiment infrastructure' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must freeze experiment-only boundary"
  exit 1
}
rg -n '`read_document`' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention read_document surface"
  exit 1
}
rg -n '`web_search`' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must mention web_search surface"
  exit 1
}
rg -n 'filesystem-like MCP|filesystem-like backend|filesystem-like MCP source backend' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must freeze filesystem-style source backend intent"
  exit 1
}
rg -n 'Obsidian' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must explicitly mark Obsidian out of scope"
  exit 1
}
rg -n 'not a new Assay product capability|not a general Assay product feature|not a product feature' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must forbid product-scope creep"
  exit 1
}

echo "[review] done"
