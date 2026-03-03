#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026q1.md"
  "docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-CONTRACT.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: sink-failure Step1 must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in sink-failure Step1: $f"
    exit 1
  fi
done

echo "[review] marker checks"
rg -n 'Sink Failure Variant' docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026q1.md >/dev/null || {
  echo "FAIL: plan title/marker missing"
  exit 1
}
rg -n '\bok\b|\btimeout\b|\bpartial\b' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing outcome modes"
  exit 1
}
rg -n 'success_any_sink_canary' docs/architecture/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-CONTRACT.md >/dev/null || {
  echo "FAIL: contract missing primary success metric"
  exit 1
}
rg -n 'No real TCP/HTTP internet exfiltration' docs/architecture/PLAN-EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026q1.md >/dev/null || {
  echo "FAIL: plan missing bounded-claim non-goal"
  exit 1
}

echo "[review] done"
