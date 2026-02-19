#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/adr025-soak-enforce.sh"
  "scripts/ci/test-adr025-soak-enforce.sh"
  "scripts/ci/fixtures/adr025/"
  "scripts/ci/review-adr025-i1-step4-b.sh"
  ".github/workflows/release.yml"
)

map_changed() {
  git diff --name-only "$BASE_REF"...HEAD
}

echo "[review] allowlist"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Step4B: $f"
    exit 1
  fi
done < <(map_changed)

echo "[review] release workflow trigger remains non-PR"
if rg -n '^\s*pull_request' .github/workflows/release.yml >/dev/null; then
  echo "FAIL: release workflow must not include pull_request trigger"
  exit 1
fi

echo "[review] only release workflow touched under .github/workflows"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* && "$f" != ".github/workflows/release.yml" ]]; then
    echo "FAIL: Step4B must not change non-release workflows ($f)"
    exit 1
  fi
done < <(map_changed)

echo "[review] ensure release workflow calls enforcement script"
rg -n "adr025-soak-enforce\.sh" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing adr025-soak-enforce.sh step"
  exit 1
}

echo "[review] ensure policy file referenced"
rg -n "schemas/soak_readiness_policy_v1\.json" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow must reference soak_readiness_policy_v1.json"
  exit 1
}

echo "[review] ensure fail-closed only in release path"
if rg -n "adr025-soak-enforce\.sh" .github/workflows/*.yml | rg -v "^\.github/workflows/release\.yml:" >/dev/null; then
  echo "FAIL: enforcement script must only be wired in release workflow for Step4B"
  exit 1
fi

echo "[review] ensure Step4B does not broaden release id-token permission"
if git diff -U0 "$BASE_REF"...HEAD -- .github/workflows/release.yml | rg -n '^\+\s*id-token:\s*write' >/dev/null; then
  echo "FAIL: Step4B must not add id-token: write in release workflow"
  exit 1
fi

echo "[review] done"
