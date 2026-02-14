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

rg_bin="$(command -v rg)"
verify_facade="crates/assay-registry/src/verify.rs"
verify_root="crates/assay-registry/src/verify_internal"

# Commit A is documentation + gate scaffolding only.
# Until Commit B performs the mechanical rename/move, this is expected to fail.
if [ ! -d "${verify_root}" ]; then
  echo "Step3 precondition not met: missing ${verify_root}"
  echo "Expected in Commit A; should pass after Commit B mechanical rename/move."
  exit 1
fi

strip_code_only() {
  local file="$1"
  awk '
    BEGIN {
      pending_cfg_test = 0
      skip_tests = 0
      depth = 0
    }
    {
      line = $0

      if (skip_tests) {
        opens = gsub(/\{/, "{", line)
        closes = gsub(/\}/, "}", line)
        depth += opens - closes
        if (depth <= 0) {
          skip_tests = 0
          depth = 0
        }
        next
      }

      if (pending_cfg_test) {
        if (line ~ /^[[:space:]]*#\[/ || line ~ /^[[:space:]]*$/) {
          next
        }
        if (line ~ /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*\{[[:space:]]*$/) {
          skip_tests = 1
          depth = 1
          pending_cfg_test = 0
          next
        }
        pending_cfg_test = 0
      }

      if (line ~ /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/) {
        pending_cfg_test = 1
        next
      }

      print
    }
  ' "$file"
}

check_no_match_code_only() {
  local pattern="$1"
  local file="$2"
  if strip_code_only "$file" | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" >/dev/null; then
    echo "forbidden code-only match in ${file}: ${pattern}"
    exit 1
  fi
}

check_only_file_matches() {
  local pattern="$1"
  local root="$2"
  local allowed="$3"
  local matches leaked
  matches="$($rg_bin -n "$pattern" "$root" -g'*.rs' || true)"
  if [ -z "$matches" ]; then
    echo "expected at least one match for: $pattern"
    exit 1
  fi
  leaked="$(echo "$matches" | "$rg_bin" -v "$allowed" || true)"
  if [ -n "$leaked" ]; then
    echo "forbidden match outside allowed file:"
    echo "$leaked"
    exit 1
  fi
}

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected delegation pattern in ${file}: ${pattern}"
    exit 1
  fi
}

echo "== Wave5 Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo check -p assay-registry

echo "== Wave5 Step3 contract anchors (verify) =="
for test_name in \
  test_verify_pack_fail_closed_matrix_contract \
  test_verify_pack_malformed_signature_reason_is_stable \
  test_verify_pack_canonicalization_equivalent_yaml_variants_contract \
  test_verify_pack_uses_canonical_bytes \
  test_verify_digest_mismatch \
  test_parse_dsse_envelope_invalid_base64
 do
  echo "anchor: ${test_name}"
  cargo test -p assay-registry "${test_name}" -- --nocapture
done

echo "== Wave5 Step3 facade gates =="
check_no_match_code_only '^\s*#\[cfg\(test\)\]|^\s*mod\s+tests\s*\{' "${verify_facade}"
check_no_match_code_only \
  'base64::|ed25519_dalek|serde_json::from_(slice|str)|parse_yaml_strict|to_canonical_jcs_bytes|compute_canonical_digest|build_pae_impl\(|verify_single_signature_impl\(|verify_dsse_signature_bytes_impl\(' \
  "${verify_facade}"

check_has_match 'verify_internal::policy::verify_pack_impl' "${verify_facade}"
check_has_match 'verify_internal::digest::verify_digest_impl' "${verify_facade}"
check_has_match 'verify_internal::digest::compute_digest_impl' "${verify_facade}"
check_has_match 'verify_internal::digest::compute_digest_strict_impl' "${verify_facade}"
check_has_match 'verify_internal::digest::compute_digest_raw_impl' "${verify_facade}"
check_has_match 'verify_internal::keys::compute_key_id_impl' "${verify_facade}"
check_has_match 'verify_internal::keys::compute_key_id_from_key_impl' "${verify_facade}"

echo "== Wave5 Step3 single-source gates =="
check_only_file_matches \
  'VerifyResult[[:space:]]*\{' \
  "${verify_root}" \
  'verify_internal/policy.rs'

check_only_file_matches \
  'build_pae_impl\(|verify_single_signature_impl\(|Signature::from_slice|key\.verify\(' \
  "${verify_root}" \
  'verify_internal/dsse.rs'

check_only_file_matches \
  'canonicalize_for_dsse_impl\(|parse_yaml_strict|to_canonical_jcs_bytes|compute_canonical_digest' \
  "${verify_root}" \
  'verify_internal/digest.rs|verify_internal/tests.rs'

echo "== Wave5 Step3 policy/dsse boundary gates =="
check_no_match_code_only \
  'base64::|ed25519_dalek|serde_json::from_(slice|str)|Signature::from_slice|Verifier|build_pae_impl\(|verify_single_signature_impl\(' \
  "${verify_root}/policy.rs"
check_no_match_code_only \
  'allow_unsigned|skip_signature|Unsigned|VerifyOptions|policy' \
  "${verify_root}/dsse.rs"

policy_boundary_calls="$(rg -n 'verify_dsse_signature_bytes_impl\(' "${verify_root}/policy.rs" || true)"
policy_boundary_count="$(echo "$policy_boundary_calls" | sed '/^$/d' | wc -l | tr -d ' ')"
if [ "$policy_boundary_count" -ne 1 ]; then
  echo "expected exactly one DSSE boundary call in policy.rs, got ${policy_boundary_count}"
  echo "$policy_boundary_calls"
  exit 1
fi

echo "== Wave5 Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-registry/src/verify.rs$|^crates/assay-registry/src/verify_internal/|^docs/contributing/SPLIT-MOVE-MAP-wave5-step3-verify.md$|^docs/contributing/SPLIT-CHECKLIST-wave5-step3-verify.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave5-step3-verify.md$|^scripts/ci/review-wave5-step3.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave5 Step3 reviewer script: PASS"
