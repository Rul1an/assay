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
legacy_file="crates/assay-registry/src/verify_internal/tests.rs"

count_delta_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${legacy_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" crates/assay-registry/src/verify_internal/tests -g '*.rs' || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave T1 B2 quality checks =="
cargo fmt --check
cargo clippy -p assay-registry --tests -- -D warnings
cargo test -p assay-registry verify_internal

echo "== Wave T1 B2 anchor checks =="
for name in \
  test_verify_pack_fail_closed_matrix_contract \
  test_verify_pack_malformed_signature_reason_is_stable \
  test_verify_pack_uses_canonical_bytes \
  test_verify_pack_canonicalization_equivalent_yaml_variants_contract

do
  if ! "${rg_bin}" -n "^fn ${name}\(" crates/assay-registry/src/verify_internal/tests -g '*.rs' >/dev/null; then
    echo "missing expected test function: ${name}"
    exit 1
  fi
done

if [ "$("${rg_bin}" -n '^fn create_signed_envelope\(' crates/assay-registry/src/verify_internal/tests -g '*.rs' | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "create_signed_envelope helper must be single-source"
  exit 1
fi

echo "== Wave T1 B2 drift gates =="
count_delta_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
count_delta_no_increase '\bunsafe\b' 'unsafe'
count_delta_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
count_delta_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave T1 B2 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-registry/src/verify_internal/tests.rs$|^crates/assay-registry/src/verify_internal/tests/|^docs/contributing/SPLIT-CHECKLIST-wave-t1-b2-verify-internal.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-t1-b2-verify-internal.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b2-verify-internal.md$|^scripts/ci/review-wave-t1-b2-verify-internal.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave T1 B2"
  exit 1
fi

echo "Wave T1 B2 reviewer script: PASS"
