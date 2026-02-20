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
  "docs/architecture/PLAN-ADR-025-I2-audit-kit-closure-2026q2.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr025-i2-step4-c.sh"
)

echo "[review] diff allowlist"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step4C must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I2 Step4C: $f"
    exit 1
  fi
done

echo "[review] invariants on main assets"
test -f schemas/closure_release_policy_v1.json || { echo "FAIL: missing closure_release_policy_v1.json"; exit 1; }
test -f scripts/ci/adr025-closure-release.sh || { echo "FAIL: missing adr025-closure-release.sh"; exit 1; }
test -f scripts/ci/review-adr025-i2-step4-b.sh || { echo "FAIL: missing review-adr025-i2-step4-b.sh"; exit 1; }
test -f .github/workflows/release.yml || { echo "FAIL: missing release.yml"; exit 1; }

rg -n "adr025-closure-release\.sh" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must reference adr025-closure-release.sh"
  exit 1
}
rg -n "closure_release_policy_v1\.json" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must reference closure_release_policy_v1.json"
  exit 1
}

echo "[review] done"
