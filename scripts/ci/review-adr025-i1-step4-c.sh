#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-ADR-025-I1-audit-kit-soak-2026q2.md"
  "docs/ROADMAP.md"
  "docs/ops/ADR-025-SOAK-ENFORCEMENT-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-adr025-step4-c-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-step4-c-closure.md"
  "scripts/ci/review-adr025-i1-step4-c.sh"
)

echo "[review] allowlist diff vs ${BASE_REF}"
changed="$(git diff --name-only "$BASE_REF"...HEAD)"

while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step4C must not edit workflows ($f)"
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
    echo "FAIL: file not allowed in Step4C: $f"
    exit 1
  fi
done <<< "$changed"

echo "[review] Step4B invariant checks from repo state"

test -f scripts/ci/adr025-soak-enforce.sh || { echo "FAIL: missing scripts/ci/adr025-soak-enforce.sh"; exit 1; }
rg -n 'classifier_version' scripts/ci/adr025-soak-enforce.sh >/dev/null || { echo "FAIL: enforcement script missing classifier lock"; exit 1; }
rg -n 'runs_observed_minimum' scripts/ci/adr025-soak-enforce.sh >/dev/null || { echo "FAIL: enforcement script missing minimum window logic"; exit 1; }

rg -n 'ADR-025 enforce readiness \(fail-closed\)' .github/workflows/release.yml >/dev/null || { echo "FAIL: release.yml missing ADR-025 enforcement step"; exit 1; }
rg -n 'adr025-nightly-readiness\.yml' .github/workflows/release.yml >/dev/null || { echo "FAIL: release.yml missing readiness workflow reference"; exit 1; }
rg -n 'nightly_readiness\.json' .github/workflows/release.yml >/dev/null || { echo "FAIL: release.yml missing readiness artifact JSON check"; exit 1; }
rg -n 'schemas/soak_readiness_policy_v1\.json' .github/workflows/release.yml >/dev/null || { echo "FAIL: release.yml missing policy reference"; exit 1; }

if rg -n '^\s*pull_request' .github/workflows/release.yml >/dev/null; then
  echo "FAIL: release workflow must not include pull_request trigger"
  exit 1
fi

echo "[review] done"
