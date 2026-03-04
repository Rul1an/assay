#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md"
  "scripts/ci/review-b4a-dx-freeze.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: B4A freeze must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in B4A freeze: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n '^# ADR-030: Coverage \+ Wrap DX Polish' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 title missing"
  exit 1
}
rg -n 'assay coverage.*--format (md\|json)|--format md' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 missing --format md contract"
  exit 1
}
rg -n -- '--declared-tools-file' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 missing --declared-tools-file contract"
  exit 1
}
rg -n -- '--coverage-out' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 missing wrap export mention (--coverage-out)"
  exit 1
}
rg -n -- '--state-window-out' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 missing wrap export mention (--state-window-out)"
  exit 1
}
rg -n 'wrapped.*exit.*authoritative|wrapped > coverage > state-window' docs/architecture/ADR-030-Coverage-Wrap-DX-Polish.md >/dev/null || {
  echo "FAIL: ADR-030 missing exit priority constraint"
  exit 1
}

echo "[review] done"
