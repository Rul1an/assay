#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/codex/wave3-step1-behavior-freeze-v2"
fi

rg_bin="$(command -v rg)"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected delegation pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_no_match_in_dir_excluding() {
  local pattern="$1"
  local root="$2"
  local excluded_file="$3"
  local matches
  matches="$($rg_bin -n "$pattern" "$root" -g'*.rs' -g"!${excluded_file}" || true)"
  if [ -n "$matches" ]; then
    echo "forbidden matches outside ${excluded_file}:"
    echo "$matches"
    exit 1
  fi
}

echo "== Wave4 Step2 quality checks =="
echo "using base_ref=${base_ref}"
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo check -p assay-registry

echo "== Wave4 Step2 contract anchors =="
for test_name in \
  test_lockfile_v2_roundtrip \
  test_lockfile_stable_ordering \
  test_lockfile_digest_mismatch_detection \
  test_lockfile_signature_fields \
  test_cache_roundtrip \
  test_cache_integrity_failure \
  test_signature_json_corrupt_handling \
  test_atomic_write_prevents_partial_cache
do
  echo "anchor: ${test_name}"
  cargo test -p assay-registry "${test_name}" -- --nocapture
done

echo "== Wave4 Step2 delegation gates =="
check_has_match 'lockfile_next::io::load_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::io::save_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::parse::parse_lockfile_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::format::to_yaml_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::format::add_pack_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::generate_lockfile_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::digest::verify_lockfile_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::digest::check_lockfile_impl' crates/assay-registry/src/lockfile.rs
check_has_match 'lockfile_next::digest::update_lockfile_impl' crates/assay-registry/src/lockfile.rs

check_has_match 'cache_next::put::put_impl' crates/assay-registry/src/cache.rs
check_has_match 'cache_next::keys::pack_dir_impl' crates/assay-registry/src/cache.rs
check_has_match 'cache_next::io::default_cache_dir_impl' crates/assay-registry/src/cache.rs
check_has_match 'cache_next::policy::parse_cache_control_expiry_impl' crates/assay-registry/src/cache.rs
check_has_match 'cache_next::integrity::parse_signature_impl' crates/assay-registry/src/cache.rs
check_has_match 'cache_next::io::write_atomic_impl' crates/assay-registry/src/cache.rs

# Lockfile facade should no longer own direct fs/logging paths for load/save.
if "$rg_bin" -n 'tokio::fs|tracing::info|fs::read_to_string|fs::write' crates/assay-registry/src/lockfile.rs; then
  echo "lockfile facade still contains direct IO/logging ownership"
  exit 1
fi

echo "== Wave4 Step2 single-source gates =="
check_no_match_in_dir_excluding 'fs::rename\(' crates/assay-registry/src/cache_next io.rs
check_no_match_in_dir_excluding 'create_dir_all\(' crates/assay-registry/src/cache_next put.rs
check_no_match_in_dir_excluding 'max-age=' crates/assay-registry/src/cache_next policy.rs
check_no_match_in_dir_excluding 'sort_by\(' crates/assay-registry/src/lockfile_next format.rs

echo "== Wave4 Step2 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-registry/src/lockfile.rs$|^crates/assay-registry/src/cache.rs$|^crates/assay-registry/src/lockfile_next/|^crates/assay-registry/src/cache_next/|^docs/contributing/SPLIT-MOVE-MAP-wave4-step2.md$|^docs/contributing/SPLIT-CHECKLIST-wave4-step2.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave4-step2.md$|^scripts/ci/review-wave4-step2.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave4 Step2 reviewer script: PASS"
