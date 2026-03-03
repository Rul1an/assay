#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md"
  "docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RERUN.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-wrap-bypass-results.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: wrap-bypass results PR must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    if [[ "$f" == "$a" ]]; then
      ok="true"
      break
    fi
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in wrap-bypass results PR: $f"
    exit 1
  fi
done

echo "[review] marker checks"
rg -n 'Repo commit \(scripts \+ binaries\): `8bf0d17ffb1d`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing commit marker"; exit 1; }
rg -n 'Cargo\.lock' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing Cargo.lock label"; exit 1; }
rg -n 'deee7ee9afa88a616118fd70dc92d269ddc6acc1a0fcd8b6ec3b3a170eadd69e' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing Cargo.lock hash"; exit 1; }
rg -n '`wrap_only`|`sequence_only`|`combined`' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing mode markers"; exit 1; }
rg -n 'first decisive block observed' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing combined interpretation marker"; exit 1; }
rg -n 'tool-mediated sink-call exfiltration control' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md >/dev/null || { echo "FAIL: missing bounded claim wording"; exit 1; }
rg -n 'RUN_LIVE=1' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RERUN.md >/dev/null || { echo "FAIL: missing live rerun instruction"; exit 1; }
rg -n 'build-info.json' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RERUN.md >/dev/null || { echo "FAIL: missing build-info reference"; exit 1; }
rg -n 'Rebuild-grade rerun checklist' docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RERUN.md >/dev/null || { echo "FAIL: missing rebuild checklist"; exit 1; }

echo "[review] done"
