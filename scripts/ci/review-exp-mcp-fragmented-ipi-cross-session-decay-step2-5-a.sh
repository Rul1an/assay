#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026q1.md"
  "docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-cross-session-decay-step2-5-a.sh"
)

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n 'k\+2' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing k+2 freeze"
  exit 1
}
rg -n 'k\+3' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing k+3 freeze"
  exit 1
}
rg -n 'Session numbering constraint \(frozen\)' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing session numbering constraint"
  exit 1
}

echo "[review] done"
