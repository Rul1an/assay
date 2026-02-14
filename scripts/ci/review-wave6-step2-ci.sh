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
release_file=".github/workflows/release.yml"

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

check_has_match_multiline() {
  local pattern="$1"
  local file="$2"
  if [ ! -f "$file" ]; then
    echo "missing expected file: ${file}"
    exit 1
  fi
  if ! "$rg_bin" -nUP -- "$pattern" "$file" >/dev/null; then
    echo "missing expected multiline pattern in ${file}: ${pattern}"
    exit 1
  fi
}

echo "== Wave6 Step2 attestation pair checks =="
check_has_match_multiline 'release:\n(?:.|\n)*?permissions:\n(?:.|\n)*?contents:\s+write\n(?:.|\n)*?attestations:\s+write\n(?:.|\n)*?id-token:\s+write' "$release_file"
check_has_match 'uses: actions/attest-build-provenance@v2' "$release_file"
check_has_match 'subject-path: release/\*' "$release_file"
check_has_match 'gh attestation verify "\$file"' "$release_file"
check_has_match '--repo "\$\{REPO\}"' "$release_file"
check_has_match '--signer-workflow "\$\{SIGNER_WORKFLOW\}"' "$release_file"
check_has_match '--cert-oidc-issuer "https://token.actions.githubusercontent.com"' "$release_file"
check_has_match 'No release archives found for attestation verification' "$release_file"
check_has_match 'Attestation verification failed for \$\{file\} after \$\{attempts\} attempts' "$release_file"

echo "== Wave6 Step2 diff allowlist =="
leaks="$($rg_bin -v \
  '^\.github/workflows/release\.yml$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|REVIEW-PACK)-wave6-step2-ci-attestation\.md$|^scripts/ci/review-wave6-step2-ci\.sh$|^docs/architecture/PLAN-split-refactor-2026q1\.md$|^docs/contributing/SPLIT-INVENTORY-wave6-step1-ci\.md$|^scripts/ci/review-wave6-step1-ci\.sh$' \
  < <(git diff --name-only "${base_ref}...HEAD") || true)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

echo "Wave6 Step2 reviewer script: PASS"
