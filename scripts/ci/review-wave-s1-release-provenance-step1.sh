#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${BASE_REF}" >&2
  exit 1
fi

release_file=".github/workflows/release.yml"
script_file="scripts/ci/release_attestation_enforce.sh"
test_file="scripts/ci/test-release-attestation-enforce.sh"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! rg -n -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

check_lacks_match() {
  local pattern="$1"
  local file="$2"
  if rg -n -- "$pattern" "$file" >/dev/null; then
    echo "unexpected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

echo "== syntax =="
bash -n "$script_file" "$test_file" "$0"

echo "== contract tests =="
bash "$test_file"

echo "== workflow YAML parse =="
ruby - <<'RUBY'
require "yaml"
YAML.load_file(".github/workflows/release.yml", permitted_classes: [], aliases: true)
puts "release workflow YAML: PASS"
RUBY

echo "== provenance policy anchors =="
check_has_match 'name: Enforce release attestation policy' "$release_file"
check_has_match 'bash scripts/ci/release_attestation_enforce\.sh' "$release_file"
check_has_match 'OUT_SUMMARY: release/assay-\$\{\{ steps\.version\.outputs\.version \}\}-release-provenance\.json' "$release_file"
check_has_match 'OUT_RAW_DIR: artifacts/release-provenance/raw' "$release_file"
check_has_match 'SOURCE_REF: \$\{\{ github\.ref \}\}' "$release_file"
check_has_match 'SOURCE_DIGEST: \$\{\{ github\.sha \}\}' "$release_file"
check_has_match 'name: Upload release provenance evidence' "$release_file"
check_has_match 'name: release-provenance-evidence' "$release_file"
check_lacks_match 'gh attestation verify "\$file"' "$release_file"

check_has_match '--source-digest "\$SOURCE_DIGEST"' "$script_file"
check_has_match '--source-ref "\$SOURCE_REF"' "$script_file"
check_has_match '--deny-self-hosted-runners' "$script_file"
check_has_match 'ATTESTATION_VERIFY_MAX_RETRIES="\$\{ATTESTATION_VERIFY_MAX_RETRIES:-5\}"' "$script_file"
check_has_match 'ATTESTATION_VERIFY_RETRY_DELAY_SECONDS="\$\{ATTESTATION_VERIFY_RETRY_DELAY_SECONDS:-5\}"' "$script_file"
check_has_match 'Attestation verification failed for \$\{asset_name\} after \$\{ATTESTATION_VERIFY_MAX_RETRIES\} attempts' "$script_file"
check_has_match 'verifiedTimestamps' "$script_file"
check_has_match 'digest\.sha256' "$script_file"

echo "== diff allowlist =="
leaks="$(rg -v \
  '^\.github/workflows/release\.yml$|^scripts/ci/release_attestation_enforce\.sh$|^scripts/ci/test-release-attestation-enforce\.sh$|^scripts/ci/review-wave-s1-release-provenance-step1\.sh$|^docs/reference/release\.md$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave-s1-release-provenance-step1\.md$' \
  < <(git diff --name-only "${BASE_REF}...HEAD") || true)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "== whitespace =="
git diff --check "${BASE_REF}...HEAD"

echo "Wave S1 Step1 reviewer script: PASS"
