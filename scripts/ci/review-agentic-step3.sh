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
mod_file='crates/assay-core/src/agentic/mod.rs'
builder_file='crates/assay-core/src/agentic/builder.rs'
helpers_file='crates/assay-core/src/agentic/policy_helpers.rs'
tests_file='crates/assay-core/src/agentic/tests/mod.rs'

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "${rg_bin}" -n "${pattern}" "${file}" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_no_match() {
  local pattern="$1"
  local file="$2"
  if "${rg_bin}" -n "${pattern}" "${file}" >/dev/null; then
    echo "forbidden pattern in ${file}: ${pattern}"
    exit 1
  fi
}

echo '== Agentic Step3 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact
cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact
cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact

echo '== Agentic Step3 closure invariants =='
mod_non_empty_loc="$(awk 'NF' "${mod_file}" | wc -l | tr -d ' ')"
if [ "${mod_non_empty_loc}" -gt 220 ]; then
  echo "facade non-empty LOC grew beyond target: ${mod_non_empty_loc} > 220"
  exit 1
fi

call_count="$(
  awk '!/^[[:space:]]*\/\//' "${mod_file}" \
    | { "${rg_bin}" -n 'builder::build_suggestions_impl\(' || true; } \
    | wc -l | tr -d ' '
)"
if [ "${call_count}" -ne 1 ]; then
  echo "expected exactly one non-comment call to builder::build_suggestions_impl, got ${call_count}"
  exit 1
fi

check_no_match '^\s*fn\s+(policy_pointers|detect_policy_shape|read_yaml|get_policy_entry|best_candidate|get_seq_strings|find_in_seq|yaml_ptr|escape_pointer|unescape_pointer)\b' "${mod_file}"
check_no_match '^\s*impl\s+(AgenticCtx|SuggestedAction|SuggestedPatch|RiskLevel|JsonPatchOp)\b' "${mod_file}"

check_has_match '^pub enum RiskLevel' "${mod_file}"
check_has_match '^pub struct SuggestedAction' "${mod_file}"
check_has_match '^pub struct SuggestedPatch' "${mod_file}"
check_has_match '^pub enum JsonPatchOp' "${mod_file}"
check_has_match '^pub struct AgenticCtx' "${mod_file}"
check_has_match '^pub fn build_suggestions\(' "${mod_file}"

check_has_match '^pub\(crate\) fn build_suggestions_impl\(' "${builder_file}"
if "${rg_bin}" -n -P '^\s*pub(?!\(crate\))' "${builder_file}"; then
  echo 'builder.rs must not expose public API beyond pub(crate)'
  exit 1
fi

test -f "${helpers_file}"
test -f "${tests_file}"

check_has_match '^fn test_deduplication\(' "${tests_file}"
check_has_match '^fn test_unknown_tool_action_only\(' "${tests_file}"
check_has_match '^fn test_rename_field_patch\(' "${tests_file}"
check_has_match '^fn test_detect_policy_shape\(' "${tests_file}"
check_has_match '^fn test_tool_poisoning_action_uses_assay_config_not_policy\(' "${tests_file}"

echo '== Agentic Step3 diff allowlist =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-agentic-step3.md$|^docs/contributing/SPLIT-REVIEW-PACK-agentic-step3.md$|^scripts/ci/review-agentic-step3.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in agentic Step3'
  exit 1
fi

echo 'Agentic Step3 reviewer script: PASS'
