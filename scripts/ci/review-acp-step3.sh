#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/codex/wave14-acp-step2-mechanical"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

echo '== ACP Step3 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-acp-step3\.md$|^docs/contributing/SPLIT-REVIEW-PACK-acp-step3\.md$|^scripts/ci/review-acp-step3\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in ACP Step3'
  exit 1
fi

if git status --porcelain -- crates/assay-adapter-acp/src | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-adapter-acp/src/** are forbidden in Step3'
  exit 1
fi

echo '== ACP Step3 quality checks =='
cargo fmt --check
cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-acp

echo '== ACP Step3 facade invariants =='
facade='crates/assay-adapter-acp/src/lib.rs'
facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 220 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 220): ${facade}"
  exit 1
fi

call_count="$("${rg_bin}" -n 'adapter_impl::convert_impl\(' "${facade}" | "${rg_bin}" -v '^\s*//' | wc -l | tr -d ' ')"
if [ "${call_count}" -ne 1 ]; then
  echo "expected exactly one non-comment call-site to adapter_impl::convert_impl(...), got ${call_count}"
  exit 1
fi

if "${rg_bin}" -n '^\s*mod tests\s*\{' "${facade}" >/dev/null; then
  echo 'inline mod tests { ... } is forbidden in lib.rs'
  exit 1
fi

if "${rg_bin}" -n '^\s*match\s+' "${facade}" >/dev/null; then
  echo 'mapping logic (match) must not remain in lib.rs facade'
  "${rg_bin}" -n '^\s*match\s+' "${facade}"
  exit 1
fi

echo '== ACP Step3 visibility invariants =='
bad_pub="$({ "${rg_bin}" -n '^\s*pub(\s|\()' crates/assay-adapter-acp/src/adapter_impl/*.rs | "${rg_bin}" -v 'pub\(crate\)' || true; })"
if [ -n "${bad_pub}" ]; then
  echo 'adapter_impl exports must be pub(crate) only:'
  echo "${bad_pub}"
  exit 1
fi

echo '== ACP Step3 test invariants =='
"${rg_bin}" -n '^\s*fn strict_happy_fixture_emits_deterministic_event\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn strict_checkout_fixture_preserves_attributes_without_lossiness\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn strict_attribute_order_normalizes_payload_but_keeps_raw_byte_hash_boundary\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn strict_missing_required_field_fails_with_measurement_error\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn lenient_invalid_event_type_emits_generic_event_and_lossiness\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn malformed_json_fails_in_all_modes\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn oversized_payload_fails_measurement_contract\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn invalid_utf8_payload_fails_measurement_contract\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn excessive_json_depth_fails_measurement_contract\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn excessive_array_length_fails_measurement_contract\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null
"${rg_bin}" -n '^\s*fn strict_unknown_top_level_fields_account_for_lossiness\(' crates/assay-adapter-acp/src/tests/mod.rs >/dev/null

echo 'ACP Step3 reviewer script: PASS'
