#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/adr025-closure-release.sh"
  "scripts/ci/test-adr025-closure-release.sh"
  "scripts/ci/fixtures/adr025-i2/"
  "scripts/ci/review-adr025-i2-step4-b.sh"
  ".github/workflows/release.yml"
)

is_allowed() {
  local f="$1"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && return 0
    else
      [[ "$f" == "$p" ]] && return 0
    fi
  done
  return 1
}

changed="$(git diff --name-only "$BASE_REF"...HEAD)"
untracked="$(git ls-files --others --exclude-standard)"

if [[ -n "$untracked" ]]; then
  if [[ -n "$changed" ]]; then
    changed="$(printf "%s\n%s\n" "$changed" "$untracked")"
  else
    changed="$untracked"
  fi
fi

if [[ -z "$changed" ]]; then
  echo "FAIL: no changes detected vs $BASE_REF"
  exit 1
fi

echo "[review] allowlist"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if ! is_allowed "$f"; then
    echo "FAIL: file not allowed in I2 Step4B: $f"
    exit 1
  fi

  if [[ "$f" == .github/workflows/* && "$f" != ".github/workflows/release.yml" ]]; then
    echo "FAIL: Step4B must not touch non-release workflows ($f)"
    exit 1
  fi
done <<< "$changed"

echo "[review] release workflow trigger remains non-PR"
if rg -n '^\s*pull_request|^\s*pull_request_target' .github/workflows/release.yml >/dev/null; then
  echo "FAIL: release workflow must not include pull_request triggers"
  exit 1
fi

echo "[review] closure release script wired in release.yml"
rg -n "adr025-closure-release\.sh" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing adr025-closure-release.sh step"
  exit 1
}

rg -n "MODE:\s*\$\{\{\s*vars\.ASSAY_CLOSURE_GATE\s*\|\|\s*'attach'\s*\}\}" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing default attach mode expression"
  exit 1
}

rg -n "name:\s*adr025-closure-release-evidence" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing closure release evidence artifact name"
  exit 1
}

echo "[review] Step4B must not add id-token: write"
if git diff -U0 "$BASE_REF"...HEAD -- .github/workflows/release.yml | rg -n '^\+\s*id-token:\s*write' >/dev/null; then
  echo "FAIL: Step4B must not add id-token: write"
  exit 1
fi

echo "[review] run script tests"
bash scripts/ci/test-adr025-closure-release.sh

echo "[review] done"
