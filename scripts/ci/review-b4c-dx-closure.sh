#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-b4-dx-polish-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-b4-dx-polish-closure.md"
  "scripts/ci/review-b4c-dx-closure.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: B4C closure must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in B4C closure: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'Coverage.*Wrap.*DX.*Runbook|COVERAGE-AND-WRAP-DX-RUNBOOK' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook title marker missing"
  exit 1
}
rg -n 'assay coverage.*--format md|--format md' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing --format md usage"
  exit 1
}
rg -n -- '--declared-tools-file' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing --declared-tools-file usage"
  exit 1
}
rg -n 'assay mcp wrap.*--coverage-out' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing wrap --coverage-out usage"
  exit 1
}
rg -n 'assay mcp wrap.*--state-window-out' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing wrap --state-window-out usage"
  exit 1
}
rg -n 'wrapped > coverage > state-window|wrapped.*authoritative' docs/ops/COVERAGE-AND-WRAP-DX-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing exit priority invariant"
  exit 1
}

echo "[review] done"
