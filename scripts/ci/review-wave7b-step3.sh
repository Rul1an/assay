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
loader_facade="crates/assay-evidence/src/lint/packs/loader.rs"
loader_root="crates/assay-evidence/src/lint/packs/loader_internal"
store_facade="crates/assay-core/src/storage/store.rs"
store_root="crates/assay-core/src/storage/store_internal"

if [ ! -d "${loader_root}" ]; then
  echo "Step3 precondition not met: missing ${loader_root}"
  exit 1
fi
if [ ! -d "${store_root}" ]; then
  echo "Step3 precondition not met: missing ${store_root}"
  exit 1
fi

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

echo "== Wave7B Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-evidence -p assay-core --all-targets -- -D warnings
cargo check -p assay-evidence -p assay-core

echo "== Wave7B Step3 contract anchors (loader) =="
for test_name in \
  lint::packs::loader::loader_internal::tests::test_local_pack_resolution \
  lint::packs::loader::loader_internal::tests::test_builtin_wins_over_local \
  lint::packs::loader::loader_internal::tests::test_local_invalid_yaml_fails \
  lint::packs::loader::loader_internal::tests::test_resolution_order_mock \
  lint::packs::loader::loader_internal::tests::test_path_wins_over_builtin \
  lint::packs::loader::loader_internal::tests::test_symlink_escape_rejected
do
  echo "anchor: ${test_name}"
  cargo test -p assay-evidence --lib "${test_name}" -- --exact
done

echo "== Wave7B Step3 contract anchors (store) =="
cargo test -p assay-core --test storage_smoke test_storage_smoke_lifecycle -- --exact
for test_name in \
  e1_runs_write_contract_insert_and_create \
  e1_latest_run_selection_is_id_based_not_timestamp_string \
  e1_stats_read_compat_keeps_legacy_started_at
do
  echo "anchor: ${test_name}"
  cargo test -p assay-core --test store_consistency_e1 "${test_name}" -- --exact
done

echo "== Wave7B Step3 facade gates =="
check_has_match 'loader_internal::run::load_pack_impl' "${loader_facade}"
check_has_match 'loader_internal::run::load_packs_impl' "${loader_facade}"
check_has_match 'loader_internal::run::load_pack_from_file_impl' "${loader_facade}"
check_no_match '^\s*#\[cfg\(test\)\]' "${loader_facade}"
check_no_match '^\s*mod\s+tests\s*[{;]' "${loader_facade}"
check_no_match '^fn\s+' "${loader_facade}"
check_no_match 'serde_yaml::from_str|serde_jcs::to_string|Sha256::digest|hex::encode' "${loader_facade}"

check_has_match 'store_internal::schema::migrate_v030_impl' "${store_facade}"
check_has_match 'store_internal::results::row_to_test_result_impl' "${store_facade}"
check_has_match 'store_internal::episodes::load_episode_graph_for_episode_id_impl' "${store_facade}"

echo "== Wave7B Step3 single-source gates =="
check_only_file_matches \
  'fn get_builtin_pack_with_name_impl\(|fn try_load_from_config_dir_impl\(|fn get_config_pack_dir_impl\(|fn is_valid_pack_name_impl\(|fn suggest_similar_pack_impl\(|fn levenshtein_distance_impl\(' \
  "${loader_root}" \
  'loader_internal/resolve.rs'
check_only_file_matches \
  'fn load_pack_from_string_impl\(|fn format_yaml_error_impl\(' \
  "${loader_root}" \
  'loader_internal/parse.rs'
check_only_file_matches \
  'fn compute_pack_digest_impl\(' \
  "${loader_root}" \
  'loader_internal/digest.rs'
check_only_file_matches \
  'fn check_version_compatibility_impl\(|fn version_satisfies_impl\(' \
  "${loader_root}" \
  'loader_internal/compat.rs'
check_only_file_matches \
  'fn load_pack_impl\(|fn load_packs_impl\(|fn load_pack_from_file_impl\(' \
  "${loader_root}" \
  'loader_internal/run.rs'
check_only_file_matches \
  '^fn test_.*' \
  "${loader_root}" \
  'loader_internal/tests.rs'

check_only_file_matches \
  'migrate_v030_impl\(|get_columns_impl\(|add_column_if_missing_impl\(|PRAGMA table_info\(|ALTER TABLE .* ADD COLUMN' \
  "${store_root}" \
  'store_internal/schema.rs'
check_only_file_matches \
  'status_to_outcome_impl\(|row_to_test_result_impl\(|insert_run_row_impl\(|message_and_details_from_attempts_impl\(|parse_attempts_impl\(' \
  "${store_root}" \
  'store_internal/results.rs'
check_only_file_matches \
  'load_episode_graph_for_episode_id_impl\(|FROM steps|FROM tool_calls' \
  "${store_root}" \
  'store_internal/episodes.rs'

echo "== Wave7B Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-evidence/src/lint/packs/loader.rs$|^crates/assay-evidence/src/lint/packs/loader_internal/|^crates/assay-core/src/storage/store.rs$|^crates/assay-core/src/storage/store_internal/|^docs/contributing/SPLIT-CHECKLIST-wave7b-step2-loader-store.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7b-step2-loader-store.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7b-step2-loader-store.md$|^scripts/ci/review-wave7b-step2.sh$|^docs/contributing/SPLIT-CHECKLIST-wave7b-step3-loader-store.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7b-step3-loader-store.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7b-step3-loader-store.md$|^scripts/ci/review-wave7b-step3.sh$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave7B Step3 reviewer script: PASS"
