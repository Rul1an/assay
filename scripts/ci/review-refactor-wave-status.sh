#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/REFACTOR-WAVE-STATUS.md"
  "docs/contributing/SPLIT-CHECKLIST-refactor-wave-status.md"
  "docs/contributing/SPLIT-REVIEW-PACK-refactor-wave-status.md"
  "scripts/ci/review-refactor-wave-status.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: refactor-wave-status slice must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in refactor-wave-status slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"

rg -n '^# Refactor Wave Status$' docs/contributing/REFACTOR-WAVE-STATUS.md >/dev/null || {
  echo "FAIL: missing Refactor Wave Status title"
  exit 1
}

rg -n '^## Closed-loop waves on `main`$' docs/contributing/REFACTOR-WAVE-STATUS.md >/dev/null || {
  echo "FAIL: missing closed-loop waves section"
  exit 1
}

rg -n '^## Standing refactor policy$' docs/contributing/REFACTOR-WAVE-STATUS.md >/dev/null || {
  echo "FAIL: missing standing refactor policy section"
  exit 1
}

rg -n '^## Definition of done$' docs/contributing/REFACTOR-WAVE-STATUS.md >/dev/null || {
  echo "FAIL: missing definition of done section"
  exit 1
}

echo "[review] done"
