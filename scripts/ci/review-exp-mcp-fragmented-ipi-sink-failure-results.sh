#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-results.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done

RESULTS=docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RESULTS.md
RERUN=docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RERUN.md

rg -n 'Sink Failure Variant|sink failure' "$RESULTS" >/dev/null || { echo "FAIL: missing title markers"; exit 1; }
rg -n 'primary_only|alt_only|mixed' "$RESULTS" >/dev/null || { echo "FAIL: missing sink path markers"; exit 1; }
rg -n 'blocked_by_wrap|blocked_by_sequence' "$RESULTS" >/dev/null || { echo "FAIL: missing attribution markers"; exit 1; }
rg -n 'success_any_sink_canary' "$RESULTS" "$RERUN" >/dev/null || { echo "FAIL: missing attempt-metric marker"; exit 1; }
rg -n 'build-info.json' "$RERUN" >/dev/null || { echo "FAIL: missing build-info reference"; exit 1; }

echo "[review] done"
