#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

DOC_PLAN="docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026q1.md"
DOC_CONTRACT="docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-CONTRACT.md"

ALLOWLIST=(
  "$DOC_PLAN"
  "$DOC_CONTRACT"
  "scripts/ci/review-exp-mcp-fragmented-ipi-wrap-bypass-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: wrap-bypass Step1 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in wrap-bypass Step1: $f"
    exit 1
  fi
done

echo "[review] required docs exist"
test -f "$DOC_PLAN" || { echo "FAIL: missing $DOC_PLAN"; exit 1; }
test -f "$DOC_CONTRACT" || { echo "FAIL: missing $DOC_CONTRACT"; exit 1; }

echo "[review] marker checks"
rg -n 'Multi-step sink leakage' "$DOC_PLAN" >/dev/null || {
  echo "FAIL: plan missing multi-step sink leakage marker"
  exit 1
}
rg -n 'No taint tracking|No taint|no taint tracking|no taint' "$DOC_PLAN" >/dev/null || {
  echo "FAIL: plan must explicitly keep no-taint boundary"
  exit 1
}
rg -n '`wrap_only`' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing wrap_only mode marker"
  exit 1
}
rg -n '`sequence_only`' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing sequence_only mode marker"
  exit 1
}
rg -n '`combined`' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: missing combined mode marker"
  exit 1
}
rg -n 'web_search\(args\.query=.*\)' "$DOC_PLAN" "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: docs must use explicit web_search(args.query=...) notation"
  exit 1
}
rg -n 'reconstructed from ordered sink queries' "$DOC_CONTRACT" >/dev/null || {
  echo "FAIL: contract missing ordered reconstruction success definition"
  exit 1
}

echo "[review] done"
