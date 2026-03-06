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

file="crates/assay-core/src/runtime/mandate_store.rs"
rg_bin="$(command -v rg)"

count_in_ref_code_only() {
  local pattern="$1"
  git show "${base_ref}:${file}" \
    | awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' \
    | "${rg_bin}" -n "${pattern}" || true
}

count_in_worktree_code_only() {
  local pattern="$1"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "${file}" \
    | "${rg_bin}" -n "${pattern}" || true
}

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$(count_in_ref_code_only "${pattern}" | wc -l | tr -d ' ')"
  after="$(count_in_worktree_code_only "${pattern}" | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Mandate Store Step 1 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Mandate Store Step 1 freeze gate (no target file edits) =="
if ! git diff --quiet "${base_ref}...HEAD" -- "${file}"; then
  echo "${file} changed in Step 1; this step must be docs/gates only"
  git diff -- "${file}" | sed -n '1,160p'
  exit 1
fi

echo "== Mandate Store Step 1 drift gates (code-only) =="
check_no_increase 'unwrap\(|expect\(' 'mandate_store unwrap/expect (code-only)'
check_no_increase '\bunsafe\b' 'mandate_store unsafe'
check_no_increase 'tokio::spawn' 'mandate_store tokio spawn'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'mandate_store panic/todo/unimplemented (code-only)'

echo "== Mandate Store Step 1 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-mandate-store-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step1.md$|^scripts/ci/review-mandate-store-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in mandate-store Step 1"
  exit 1
fi

echo "Mandate Store Step 1 reviewer script: PASS"
