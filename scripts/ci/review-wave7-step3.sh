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
authz_facade="crates/assay-core/src/runtime/authorizer.rs"
authz_root="crates/assay-core/src/runtime/authorizer_internal"

if [ ! -d "${authz_root}" ]; then
  echo "Step3 precondition not met: missing ${authz_root}"
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
        if (line ~ /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*;[[:space:]]*$/) {
          pending_cfg_test = 0
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

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
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

echo "== Wave7 Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Wave7 Step3 contract anchors =="
for test_name in \
  test_authorize_rejects_expired \
  test_authorize_rejects_not_yet_valid \
  test_authorize_rejects_tool_not_in_scope \
  test_authorize_rejects_transaction_ref_mismatch \
  test_authorize_rejects_revoked_mandate \
  test_multicall_produces_monotonic_counts_no_gaps \
  test_multicall_idempotent_same_tool_call_id \
  test_revocation_roundtrip \
  test_compute_use_id_contract_vector
do
  echo "anchor: ${test_name}"
  cargo test -p assay-core --lib "${test_name}" -- --nocapture
done
cargo test -p assay-core --test mandate_store_concurrency \
  test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Wave7 Step3 facade gates =="
check_no_match_code_only \
  '^\s*#\[cfg\(test\)\]|^\s*mod\s+tests\s*[{;]|^\s*#\[cfg\(test\)\]\s*mod\s+tests\s*;' \
  "${authz_facade}"
check_no_match_code_only \
  'mandate_store_next|\bBEGIN\b|\bCOMMIT\b|\bROLLBACK\b|tokio::spawn|spawn_blocking|tokio::process|std::process::Command|Command::new|rusqlite::|serde_jcs|sha2::|hex::encode|get_revoked_at|upsert_mandate|consume_mandate|authorizer_(next|internal)::policy::' \
  "${authz_facade}"
check_has_match 'authorizer_internal::run::authorize_and_consume_impl' "${authz_facade}"
check_has_match 'authorizer_internal::run::authorize_at_impl' "${authz_facade}"

leaked_next_refs="$(rg -n 'authorizer_next::|authorizer_next/' crates/assay-core/src/runtime -g'*.rs' || true)"
if [ -n "$leaked_next_refs" ]; then
  echo "forbidden authorizer_next references after Step3 closure:"
  echo "$leaked_next_refs"
  exit 1
fi

echo "== Wave7 Step3 single-source gates =="
check_only_file_matches \
  'policy::check_validity_window_impl|policy::check_context_impl|policy::check_scope_impl|policy::check_operation_class_impl|policy::check_transaction_ref_impl|store::check_revocation_impl|store::upsert_mandate_metadata_impl|store::consume_mandate_impl' \
  "${authz_root}" \
  'authorizer_internal/run.rs'

check_only_file_matches \
  'get_revoked_at\(|upsert_mandate\(|consume_mandate\(' \
  "${authz_root}" \
  'authorizer_internal/store.rs'

check_only_file_matches \
  'serde_jcs|sha2::|hex::encode|compute_transaction_ref_impl\(' \
  "${authz_root}" \
  'authorizer_internal/policy.rs|authorizer_internal/tests.rs'

check_no_match_code_only \
  'get_revoked_at|upsert_mandate|consume_mandate|MandateStore|ConsumeParams|MandateMetadata|BEGIN|COMMIT|ROLLBACK' \
  "${authz_root}/policy.rs"
check_no_match_code_only \
  'policy::check_validity_window_impl|policy::check_context_impl|policy::check_scope_impl|policy::check_operation_class_impl|policy::check_transaction_ref_impl|glob_matches_impl|tool_matches_scope_impl' \
  "${authz_root}/store.rs"

check_only_file_matches \
  '\bBEGIN\b|\bCOMMIT\b|\bROLLBACK\b' \
  'crates/assay-core/src/runtime/mandate_store_next' \
  'mandate_store_next/txn.rs'

txn_boundary_calls="$(rg -n 'consume_mandate_in_txn_impl\(' crates/assay-core/src/runtime/mandate_store.rs || true)"
txn_boundary_count="$(echo "$txn_boundary_calls" | sed '/^$/d' | wc -l | tr -d ' ')"
if [ "$txn_boundary_count" -ne 1 ]; then
  echo "expected exactly one consume_mandate_in_txn_impl callsite in mandate_store.rs, got ${txn_boundary_count}"
  echo "$txn_boundary_calls"
  exit 1
fi

echo "== Wave7 Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-core/src/runtime/authorizer.rs$|^crates/assay-core/src/runtime/authorizer_next/|^crates/assay-core/src/runtime/authorizer_internal/|^crates/assay-core/src/runtime/mandate_store.rs$|^crates/assay-core/src/runtime/mandate_store_next/|^docs/contributing/SPLIT-INVENTORY-wave7-step1-runtime-authz.md$|^docs/contributing/SPLIT-SYMBOLS-wave7-step1-runtime-authz.md$|^docs/contributing/SPLIT-CHECKLIST-wave7-step1-runtime-authz.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7-step1-runtime-authz.md$|^scripts/ci/review-wave7-step1.sh$|^docs/contributing/SPLIT-CHECKLIST-wave7-step2-runtime-authz.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7-step2-runtime-authz.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7-step2-runtime-authz.md$|^scripts/ci/review-wave7-step2.sh$|^docs/contributing/SPLIT-CHECKLIST-wave7-step3-runtime-authz.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7-step3-runtime-authz.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7-step3-runtime-authz.md$|^scripts/ci/review-wave7-step3.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave7 Step3 reviewer script: PASS"
