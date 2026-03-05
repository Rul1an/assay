#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/COVERAGE-V1-1-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-coverage-v1-1-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-coverage-v1-1-closure.md"
  "scripts/ci/review-coverage-v1-1-c-closure.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: coverage v1.1 closure must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in coverage v1.1 closure: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n '^# Coverage v1\.1 Runbook$' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook title missing"
  exit 1
}
rg -n -- '--out-md' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing --out-md"
  exit 1
}
rg -n -- '--routes-top' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing --routes-top"
  exit 1
}
rg -n 'coverage_report_v1' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing coverage_report_v1 reference"
  exit 1
}
rg -n 'Exit codes.*0.*2.*3|0.*success|2.*measurement|3.*infra' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing exit code contract (0/2/3)"
  exit 1
}
rg -n 'declared-tools-file|declared tools file' docs/ops/COVERAGE-V1-1-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing declared-tools-file guidance"
  exit 1
}

echo "[review] done"
