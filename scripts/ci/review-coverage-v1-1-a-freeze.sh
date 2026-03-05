#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md"
  "scripts/ci/review-coverage-v1-1-a-freeze.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows ($f)"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do [[ "$f" == "$a" ]] && ok="true" && break; done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed in coverage v1.1 A-slice: $f"; exit 1; }
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n '^# ADR-031: Coverage v1.1 DX Polish$' docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR title missing"
  exit 1
}
rg -n -- '--out-md' docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md >/dev/null || {
  echo "FAIL: missing --out-md contract"
  exit 1
}
rg -n -- '--routes-top' docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md >/dev/null || {
  echo "FAIL: missing --routes-top contract"
  exit 1
}
rg -n 'coverage_report_v1' docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md >/dev/null || {
  echo "FAIL: missing schema reference coverage_report_v1"
  exit 1
}
rg -n 'no schema bump' docs/architecture/ADR-031-Coverage-v1.1-DX-Polish.md >/dev/null || {
  echo "FAIL: missing non-goal (no schema bump)"
  exit 1
}

echo "[review] done"
