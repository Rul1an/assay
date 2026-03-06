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
mandate_file="crates/assay-core/src/runtime/mandate_store.rs"
mandate_next_root="crates/assay-core/src/runtime/mandate_store_next"
mandate_next_mod="crates/assay-core/src/runtime/mandate_store_next/mod.rs"

check_no_match() {
  local pattern="$1"
  local path="$2"
  if "${rg_bin}" -n "${pattern}" "${path}"; then
    echo "forbidden match in ${path} (pattern: ${pattern})"
    exit 1
  fi
}

check_only_file_matches() {
  local pattern="$1"
  local root="$2"
  local allowed="$3"
  local matches leaked
  matches="$("${rg_bin}" -n "${pattern}" "${root}" -g'*.rs' || true)"
  if [ -z "${matches}" ]; then
    echo "expected at least one match for: ${pattern}"
    exit 1
  fi
  leaked="$(echo "${matches}" | "${rg_bin}" -v "${allowed}" || true)"
  if [ -n "${leaked}" ]; then
    echo "forbidden match outside allowed file:"
    echo "${leaked}"
    exit 1
  fi
}

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
  ' "${file}"
}

echo "== Mandate Store Step 3 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Mandate Store Step 3 contract tests =="
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Mandate Store Step 3 closure invariants =="
if "${rg_bin}" -n '^mod tests\s*\{' "${mandate_file}"; then
  echo "inline test module must remain removed in ${mandate_file}"
  exit 1
fi
if ! "${rg_bin}" -n '^\#\[path = "mandate_store_next/tests.rs"\]$' "${mandate_file}" >/dev/null; then
  echo "missing path test include in ${mandate_file}"
  exit 1
fi
if "${rg_bin}" -n '^pub\(crate\)\s+mod\s+tests;' "${mandate_next_mod}" >/dev/null; then
  echo "duplicate module load detected in ${mandate_next_mod}"
  exit 1
fi
if ! "${rg_bin}" -n '^fn test_compute_use_id_contract_vector\(' "${mandate_next_root}/tests.rs" >/dev/null; then
  echo "contract vector test not found in moved test file"
  exit 1
fi

mandate_loc="$(wc -l < "${mandate_file}" | tr -d ' ')"
if [ "${mandate_loc}" -gt 250 ]; then
  echo "mandate facade grew beyond closure target: ${mandate_loc} > 250"
  exit 1
fi

tests_loc="$(wc -l < "${mandate_next_root}/tests.rs" | tr -d ' ')"
if [ "${tests_loc}" -lt 500 ]; then
  echo "moved tests unexpectedly shrank: ${tests_loc} < 500"
  exit 1
fi

check_only_file_matches 'BEGIN IMMEDIATE|\bCOMMIT\b|\bROLLBACK\b|transaction\(|\bTransaction\b' \
  "${mandate_next_root}" \
  'mandate_store_next/txn.rs'

strip_code_only "${mandate_file}" > /tmp/mandate_store_step3_code_only.rs
check_no_match 'INSERT INTO|UPDATE\s+\w+\s+SET|SELECT\s+.+\s+FROM|DELETE\s+FROM|BEGIN IMMEDIATE|\bCOMMIT\b|\bROLLBACK\b' /tmp/mandate_store_step3_code_only.rs

echo "== Mandate Store Step 3 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-mandate-store-step3.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step3.md$|^scripts/ci/review-mandate-store-step3.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in mandate-store Step 3"
  exit 1
fi

echo "Mandate Store Step 3 reviewer script: PASS"
