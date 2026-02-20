#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-adr025-i2-step4-c-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-i2-step4-c-closure.md"
  "scripts/ci/review-adr025-i2-stab-c.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: stabilization StepC must not touch workflows ($f)"
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
    echo "FAIL: file not allowed in stabilization StepC: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] invariants"
test -f scripts/ci/adr025-closure-release.sh || { echo "FAIL: missing scripts/ci/adr025-closure-release.sh"; exit 1; }
test -f scripts/ci/test-adr025-closure-release.sh || { echo "FAIL: missing scripts/ci/test-adr025-closure-release.sh"; exit 1; }

rg -n "ASSAY_CLOSURE_RELEASE_TEST_MODE" scripts/ci/adr025-closure-release.sh >/dev/null || {
  echo "FAIL: missing ASSAY_CLOSURE_RELEASE_TEST_MODE in closure release script"
  exit 1
}
rg -n "violations must be a list if present" scripts/ci/adr025-closure-release.sh >/dev/null || {
  echo "FAIL: missing violations type contract check in closure release script"
  exit 1
}

rg -n "Violations field wrong type" docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing violations wrong-type triage section"
  exit 1
}
rg -n "Test-only knobs" docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md >/dev/null || {
  echo "FAIL: runbook missing test-only knobs section"
  exit 1
}

echo "[review] done"
