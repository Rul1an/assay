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

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before_matches after_matches before after
  before_matches="$(
    git show "${base_ref}:${mandate_file}" \
      | strip_code_only /dev/stdin \
      | "${rg_bin}" -n "${pattern}" || true
  )"
  after_matches="$(
    strip_code_only "${mandate_file}" \
      | "${rg_bin}" -n "${pattern}" || true
  )"
  before="$(printf '%s' "${before_matches}" | wc -l | tr -d ' ')"
  after="$(printf '%s' "${after_matches}" | wc -l | tr -d ' ')"
  if [ -n "${before_matches}" ]; then
    before=$((before + 1))
  fi
  if [ -n "${after_matches}" ]; then
    after=$((after + 1))
  fi
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Mandate Store Step 2 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Mandate Store Step 2 contract tests =="
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Mandate Store Step 2 mechanical split gates =="
if "${rg_bin}" -n '^mod tests\s*\{' "${mandate_file}"; then
  echo "inline test module still present in ${mandate_file}"
  exit 1
fi
if ! "${rg_bin}" -n '^\#\[path = "mandate_store_next/tests.rs"\]$' "${mandate_file}" >/dev/null; then
  echo "missing path module wiring for moved tests in ${mandate_file}"
  exit 1
fi
if "${rg_bin}" -n '^pub\(crate\)\s+mod\s+tests;' "${mandate_next_mod}" >/dev/null; then
  echo "duplicate module load: remove mod tests from ${mandate_next_mod}"
  exit 1
fi
if ! "${rg_bin}" -n '^fn test_compute_use_id_contract_vector\(' "${mandate_next_root}/tests.rs" >/dev/null; then
  echo "moved test suite missing contract vector test"
  exit 1
fi

check_only_file_matches 'BEGIN IMMEDIATE|\bCOMMIT\b|\bROLLBACK\b|transaction\(|\bTransaction\b' \
  "${mandate_next_root}" \
  'mandate_store_next/txn.rs'

strip_code_only "${mandate_file}" > /tmp/mandate_store_step2_code_only.rs
check_no_match 'INSERT INTO|UPDATE\s+\w+\s+SET|SELECT\s+.+\s+FROM|DELETE\s+FROM|BEGIN IMMEDIATE|\bCOMMIT\b|\bROLLBACK\b' /tmp/mandate_store_step2_code_only.rs

echo "== Mandate Store Step 2 drift gates (code-only) =="
check_no_increase 'unwrap\(|expect\(' 'mandate_store unwrap/expect (code-only)'
check_no_increase '\bunsafe\b' 'mandate_store unsafe'
check_no_increase 'tokio::spawn' 'mandate_store tokio spawn'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'mandate_store panic/todo/unimplemented (code-only)'

echo "== Mandate Store Step 2 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-core/src/runtime/mandate_store.rs$|^crates/assay-core/src/runtime/mandate_store_next/mod.rs$|^crates/assay-core/src/runtime/mandate_store_next/tests.rs$|^docs/contributing/SPLIT-CHECKLIST-mandate-store-step2.md$|^docs/contributing/SPLIT-MOVE-MAP-mandate-store-step2.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step2.md$|^scripts/ci/review-mandate-store-step2.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in mandate-store Step 2"
  exit 1
fi

echo "Mandate Store Step 2 reviewer script: PASS"
