#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RERUN.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-results.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: results PR must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in results PR: $f"
    exit 1
  fi
done

echo "[review] marker checks"
rg -n 'Repo commit:\s+`289a43ecc144`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: results doc missing commit marker"
  exit 1
}
rg -n 'Runs total:\s+\*\*80\*\*' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md >/dev/null || {
  echo "FAIL: results doc missing combined runs marker"
  exit 1
}
rg -n 'bash scripts/ci/test-exp-mcp-fragmented-ipi.sh' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RERUN.md >/dev/null || {
  echo "FAIL: rerun doc missing smoke rerun instruction"
  exit 1
}
rg -n 'RUNS_ATTACK=10 RUNS_LEGIT=10 RUN_SET=deterministic' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RERUN.md >/dev/null || {
  echo "FAIL: rerun doc missing full rerun instructions"
  exit 1
}

echo "[review] done"
