#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "scripts/ci/adr025-closure-release.sh"
  "scripts/ci/test-adr025-closure-release.sh"
  "scripts/ci/review-adr025-i2-stab-d.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: stabilization StepD must not touch workflows ($f)"
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
    echo "FAIL: file not allowed in stabilization StepD: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] run closure release tests"
bash scripts/ci/test-adr025-closure-release.sh

echo "[review] done"
