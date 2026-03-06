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
legacy_file="crates/assay-adapter-ucp/src/lib.rs"
new_root="crates/assay-adapter-ucp/src"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_no_match() {
  local pattern="$1"
  local file="$2"
  if "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "forbidden match in ${file}: ${pattern}"
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

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${legacy_file}" | "$rg_bin" -n "$pattern" || true; } | wc -l | tr -d ' ')"
  after="$({ "$rg_bin" -n "$pattern" "$new_root" -g'*.rs' || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave8B Step2 quality checks =="
cargo fmt --check
cargo clippy -p assay-adapter-ucp -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-ucp
bash scripts/ci/test-adapter-ucp.sh

echo "== Wave8B Step2 contract anchors =="
for test_name in \
  adapter_impl::tests::protocol_metadata_uses_frozen_release_tag \
  adapter_impl::tests::strict_order_fixture_maps_expected_event \
  adapter_impl::tests::strict_missing_order_id_fails_with_measurement_error \
  adapter_impl::tests::lenient_missing_order_id_substitutes_unknown_order \
  adapter_impl::tests::excessive_json_depth_fails_measurement_contract

do
  echo "anchor: ${test_name}"
  cargo test -p assay-adapter-ucp "${test_name}" -- --exact
done

echo "== Wave8B Step2 facade gates =="
check_has_match '^mod adapter_impl;$' 'crates/assay-adapter-ucp/src/lib.rs'
check_no_match '^fn parse_packet\(|^fn validate_protocol\(|^fn observed_version\(|^fn build_payload\(|^fn normalize_json\(' 'crates/assay-adapter-ucp/src/lib.rs'

echo "== Wave8B Step2 single-source gates =="
check_only_file_matches '^pub\(super\) fn convert\(' "$new_root" 'adapter_impl/(convert.rs|mod.rs)'
check_only_file_matches 'fn parse_packet\(|fn validate_protocol\(' "$new_root" 'adapter_impl/parse.rs'
check_only_file_matches 'fn observed_version\(|fn validate_supported_version\(' "$new_root" 'adapter_impl/version.rs'
check_only_file_matches 'fn string_field\(|fn nested_string_field\(|fn timestamp_field\(|fn default_time\(' "$new_root" 'adapter_impl/fields.rs'
check_only_file_matches 'fn map_event_type\(|fn primary_id_for_event\(|fn count_unmapped_top_level_fields\(' "$new_root" 'adapter_impl/mapping.rs'
check_only_file_matches 'fn build_payload\(|fn normalized_object_field\(|fn normalize_json\(' "$new_root" 'adapter_impl/payload.rs'
check_only_file_matches '^fn (protocol_metadata_|strict_|lenient_|malformed_|oversized_|invalid_utf8_|excessive_|reserved_key|fixture|fixture_dir)' "$new_root" 'adapter_impl/tests.rs'

echo "== Wave8B Step2 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave8B Step2 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v '^crates/assay-adapter-a2a/src/lib.rs$|^crates/assay-adapter-a2a/src/adapter_impl/|^docs/contributing/SPLIT-CHECKLIST-wave8a-step1-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-a2a.md$|^scripts/ci/review-wave8a-step1.sh$|^docs/contributing/SPLIT-MOVE-MAP-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step2-a2a.md$|^scripts/ci/review-wave8a-step2.sh$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step3-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step3-a2a.md$|^scripts/ci/review-wave8a-step3.sh$|^docs/contributing/SPLIT-CHECKLIST-wave8b-step1-ucp.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8b-step1-ucp.md$|^scripts/ci/review-wave8b-step1.sh$|^crates/assay-adapter-ucp/src/lib.rs$|^crates/assay-adapter-ucp/src/adapter_impl/|^docs/contributing/SPLIT-MOVE-MAP-wave8b-step2-ucp.md$|^docs/contributing/SPLIT-CHECKLIST-wave8b-step2-ucp.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8b-step2-ucp.md$|^scripts/ci/review-wave8b-step2.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "$rg_bin" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave8B Step2"
  exit 1
fi

echo "Wave8B Step2 reviewer script: PASS"
