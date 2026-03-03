#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-ablation-results.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ablation results PR must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ablation results PR: $f"
    exit 1
  fi
done

echo "[review] marker checks"
rg -n 'Scripts/tree commit: `dd6c0c9952a3`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing scripts/tree commit marker"; exit 1; }
rg -n 'binary provenance commit: `f4364a09a09b`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing binary provenance marker"; exit 1; }
rg -n 'tool-mediated sink-call exfiltration control' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing bounded claim wording"; exit 1; }
rg -n 'blocked_by_wrap|blocked_by_sequence' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing mechanism attribution markers"; exit 1; }
rg -n 'Rebuild-grade rerun checklist' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md >/dev/null || { echo "FAIL: missing rebuild-grade rerun checklist"; exit 1; }
rg -n 'RUN_LIVE=1' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md >/dev/null || { echo "FAIL: missing live rerun instruction"; exit 1; }

echo "[review] done"
