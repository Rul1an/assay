#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/exp-mcp-fragmented-ipi-ablation-step2-promote}"
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
rg -n 'Repo commit: `c6358730456a`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: results doc missing commit marker"; exit 1; }
rg -n 'local mock MCP harness|mock-harness result|not a live external-tool benchmark' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: results doc must state mock-harness limitation"; exit 1; }
rg -n 'wrap_only|sequence_only|combined' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: results doc missing ablation modes"; exit 1; }
rg -n 'RUNS_ATTACK=10 RUNS_LEGIT=10' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md >/dev/null || { echo "FAIL: rerun doc missing extended run command"; exit 1; }
rg -n 'protected_sequence_sidecar_enabled' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-ABLATION-2026Q1-RERUN.md >/dev/null || { echo "FAIL: rerun doc missing sidecar audit field guidance"; exit 1; }

echo "[review] done"
