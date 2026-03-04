#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026Q1-RERUN.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-cross-session-decay-results.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

RESULTS=docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026Q1-RESULTS.md
RERUN=docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026Q1-RERUN.md

rg -n 'Cross-session Decay Variant|cross-session decay' "$RESULTS" >/dev/null || { echo "FAIL: missing title markers"; exit 1; }
rg -n 'DECAY_RUNS|session2|session4|same_session_control_any_sink_canary' "$RESULTS" >/dev/null || { echo "FAIL: missing decay/control markers"; exit 1; }
rg -n 'build-info.json' "$RESULTS" "$RERUN" >/dev/null || { echo "FAIL: missing build-info reference"; exit 1; }
rg -n 'k\+1|k\+2|k\+3|first delayed sink session' "$RESULTS" "$RERUN" >/dev/null || { echo "FAIL: missing bounded horizon note"; exit 1; }

echo "[review] done"
