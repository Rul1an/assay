#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if [ ! -f "$file" ]; then
    echo "missing expected file: ${file}"
    exit 1
  fi
  if ! "$rg_bin" -n -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

echo "== Wave6 Step1 baseline anchor checks =="
check_has_match 'attestation_conditional:' .github/workflows/action-tests.yml
check_has_match 'name:[[:space:]]+Wave 0 feature matrix' .github/workflows/split-wave0-gates.yml
check_has_match 'cargo nextest run -p assay-registry --all-features' .github/workflows/split-wave0-gates.yml
check_has_match 'cargo hack' .github/workflows/split-wave0-gates.yml
check_has_match 'cargo install --locked cargo-semver-checks' .github/workflows/split-wave0-gates.yml
check_has_match '-D clippy::todo -D clippy::unimplemented' .github/workflows/split-wave0-gates.yml
check_has_match 'id-token:[[:space:]]+write' .github/workflows/release.yml

echo "== Wave6 Step1 diff allowlist =="
leaks="$($rg_bin -v \
  '^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|REVIEW-PACK)-wave6-step1-ci\.md$|^scripts/ci/review-wave6-step1-ci\.sh$|^docs/architecture/PLAN-split-refactor-2026q1\.md$' \
  < <(git diff --name-only "${base_ref}...HEAD") || true)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

echo "Wave6 Step1 reviewer script: PASS"
