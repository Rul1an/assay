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
inventory_file="scripts/ci/release_archive_inventory.sh"
attestation_file="scripts/ci/release_attestation_enforce.sh"
proof_kit_file="scripts/ci/release_proof_kit_build.sh"
proof_kit_test="scripts/ci/test-release-proof-kit-build.sh"
release_doc="docs/reference/release.md"
proof_kit_doc="docs/security/RELEASE-PROOF-KIT.md"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! rg -ni -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

check_lacks_match() {
  local pattern="$1"
  local file="$2"
  if rg -ni -- "$pattern" "$file" >/dev/null; then
    echo "unexpected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

echo "== syntax =="
bash -n "$inventory_file" "$attestation_file" "$proof_kit_file" "$proof_kit_test" "$0"

echo "== contract tests =="
bash scripts/ci/test-release-attestation-enforce.sh
bash "$proof_kit_test"

echo "== workflow YAML parse =="
ruby - <<'RUBY'
require "yaml"
YAML.load_file(".github/workflows/release.yml", permitted_classes: [], aliases: true)
puts "release workflow YAML: PASS"
RUBY

echo "== workflow anchors =="
check_has_match 'name: Build release proof kit' "$release_file"
check_has_match 'bash scripts/ci/release_proof_kit_build\.sh' "$release_file"
check_has_match 'PROVENANCE_SUMMARY: release/assay-\$\{\{ steps\.version\.outputs\.version \}\}-release-provenance\.json' "$release_file"
check_has_match 'PROVENANCE_SUMMARY_SHA256: release/assay-\$\{\{ steps\.version\.outputs\.version \}\}-release-provenance\.json\.sha256' "$release_file"
check_has_match 'OUT_ARCHIVE: release/assay-\$\{\{ steps\.version\.outputs\.version \}\}-release-proof-kit\.tar\.gz' "$release_file"

echo "== script anchors =="
check_has_match 'release_archive_inventory\.sh' "$attestation_file"
check_has_match 'gh attestation trusted-root' "$proof_kit_file"
check_has_match 'gh attestation download' "$proof_kit_file"
check_has_match 'proof kit asset set does not match S1 provenance summary' "$proof_kit_file"
check_has_match 'expected exactly one bundle file' "$proof_kit_file"
check_has_match 'release-provenance\.json' "$proof_kit_file"
check_has_match 'verify-offline\.sh' "$proof_kit_file"
check_has_match 'verify-release-online\.sh' "$proof_kit_file"
check_has_match 'canonical verification path' "$proof_kit_file"
check_has_match 'convenience-only' "$proof_kit_file"
check_has_match 'general Sigstore verification' "$proof_kit_file"

echo "== anti-drift anchors =="
check_has_match '\.verification_policy\.repo' "$proof_kit_file"
check_has_match '\.verification_policy\.signer_workflow' "$proof_kit_file"
check_has_match '\.verification_policy\.cert_oidc_issuer' "$proof_kit_file"
check_has_match '\.verification_policy\.source_ref' "$proof_kit_file"
check_has_match '\.verification_policy\.source_digest' "$proof_kit_file"
check_has_match '\.verification_policy\.predicate_type' "$proof_kit_file"

echo "== docs anchors =="
check_has_match 'release-proof-kit\.tar\.gz' "$release_doc"
check_has_match 'verify-offline\.sh --assets-dir' "$release_doc"
check_has_match 'canonical consumer verification path' "$proof_kit_doc"
check_has_match 'trusted_root\.jsonl' "$proof_kit_doc"
check_has_match 'general Sigstore verification' "$proof_kit_doc"
check_has_match 'generic Rekor verification' "$proof_kit_doc"
check_has_match 'complete supply-chain guarantee' "$proof_kit_doc"
check_has_match 'runtime trust enforcement' "$proof_kit_doc"

echo "== banned overclaim phrases =="
for file in "$release_doc" "$proof_kit_doc"; do
  check_lacks_match 'supply chain solved' "$file"
  check_lacks_match 'general Rekor verifier' "$file"
  check_lacks_match 'general Sigstore verifier' "$file"
done

echo "== diff allowlist =="
leaks="$(rg -v \
  '^\.github/workflows/release\.yml$|^scripts/ci/release_archive_inventory\.sh$|^scripts/ci/release_attestation_enforce\.sh$|^scripts/ci/release_proof_kit_build\.sh$|^scripts/ci/test-release-proof-kit-build\.sh$|^scripts/ci/review-wave-s2-release-proof-kit-step1\.sh$|^docs/reference/release\.md$|^docs/security/RELEASE-PROOF-KIT\.md$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave-s2-release-proof-kit-step1\.md$' \
  < <(git diff --name-only "${BASE_REF}...HEAD") || true)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "== whitespace =="
git diff --check "${BASE_REF}...HEAD"

echo "Wave S2 Step1 reviewer script: PASS"
