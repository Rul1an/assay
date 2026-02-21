#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/adr025-otel-release.sh"
  "scripts/ci/test-adr025-otel-release.sh"
  "scripts/ci/fixtures/adr025-i3/"
  "scripts/ci/review-adr025-i3-step4-b.sh"
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

echo "[review] allowlist"
changed="$(git diff --name-only "$BASE_REF"...HEAD)"

allowlisted_untracked=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if is_allowed "$f" || [[ "$f" == .github/workflows/* ]]; then
    if [[ -n "$allowlisted_untracked" ]]; then
      allowlisted_untracked+=$'\n'
    fi
    allowlisted_untracked+="$f"
  fi
done < <(git ls-files --others --exclude-standard)

if [[ -n "$allowlisted_untracked" ]]; then
  if [[ -n "$changed" ]]; then
    changed="$(printf "%s\n%s\n" "$changed" "$allowlisted_untracked")"
  else
    changed="$allowlisted_untracked"
  fi
fi

if [[ -z "$changed" ]]; then
  echo "FAIL: no changes detected vs $BASE_REF"
  exit 1
fi

while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if ! is_allowed "$f"; then
    echo "FAIL: file not allowed in I3 Step4B: $f"
    exit 1
  fi

  if [[ "$f" == .github/workflows/* && "$f" != ".github/workflows/release.yml" ]]; then
    echo "FAIL: Step4B must not touch non-release workflows ($f)"
    exit 1
  fi
done <<< "$changed"

echo "[review] run otel release tests"
bash scripts/ci/test-adr025-otel-release.sh

echo "[review] release workflow trigger remains non-PR"
if rg -n '^\s*pull_request|^\s*pull_request_target' .github/workflows/release.yml >/dev/null; then
  echo "FAIL: release workflow must not include pull_request triggers"
  exit 1
fi

echo "[review] otel release script wired in release.yml"
rg -n "adr025-otel-release\.sh" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing adr025-otel-release.sh step"
  exit 1
}

rg -n "MODE:\\s*\\$\\{\\{ vars\\.ASSAY_OTEL_GATE \\|\\| 'attach' \\}\\}" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing default attach mode expression"
  exit 1
}

rg -n "schemas/otel_release_policy_v1\.json" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow must reference otel_release_policy_v1.json"
  exit 1
}

rg -n "name:\s*adr025-otel-bridge-release-evidence" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow missing otel release evidence artifact name"
  exit 1
}

echo "[review] Step4B must not add id-token: write"
if git diff -U0 "$BASE_REF"...HEAD -- .github/workflows/release.yml | rg -n '^\+\s*id-token:\s*write' >/dev/null; then
  echo "FAIL: Step4B must not add id-token: write"
  exit 1
fi

echo "[review] done"
