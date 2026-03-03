#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026q1.md"
  "docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-cross-session-decay-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: cross-session decay Step1 must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in cross-session decay Step1: $f"
    exit 1
  fi
done

echo "[review] marker checks"
rg -n 'Session definition \(frozen\)' docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026q1.md >/dev/null || {
  echo "FAIL: plan missing session definition freeze"
  exit 1
}
rg -n 'run-count-based decay window' docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026q1.md >/dev/null || {
  echo "FAIL: plan missing run-count-based decay freeze"
  exit 1
}
rg -n 'DECAY_RUNS' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing DECAY_RUNS parameterization"
  exit 1
}
rg -n 'No claims about true long-term agent memory' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing bounded-claim non-goal"
  exit 1
}

echo "[review] done"
