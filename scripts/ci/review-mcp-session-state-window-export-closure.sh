#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-mcp-session-state-window-export-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-mcp-session-state-window-export-closure.md"
  "scripts/ci/review-mcp-session-state-window-export-closure.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: closure slice must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in closure slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'MCP Session/State Window Export Runbook \(v1\)' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook title missing"
  exit 1
}
rg -n 'assay mcp wrap.*--state-window-out' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: state-window-out usage missing"
  exit 1
}
rg -n 'wrapped.*exit.*authoritative|exit.*priority|wrapped > coverage > state-window' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: exit priority/authoritative note missing"
  exit 1
}
rg -n 'deterministic|canonical JSON|sha256' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: deterministic snapshot id explanation missing"
  exit 1
}
rg -n 'stores_raw_tool_args.*false' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: privacy defaults missing"
  exit 1
}
rg -n 'ADR-029' docs/ops/MCP-SESSION-STATE-WINDOW-EXPORT-RUNBOOK.md >/dev/null || {
  echo "FAIL: ADR reference missing"
  exit 1
}

echo "[review] done"
