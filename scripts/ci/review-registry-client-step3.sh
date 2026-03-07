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
facade_file="crates/assay-registry/tests/registry_client.rs"
module_root="crates/assay-registry/tests/registry_client"
mod_file="${module_root}/mod.rs"
support_file="${module_root}/support.rs"

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

echo "== Registry Client Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-registry --tests -- -D warnings
cargo test -p assay-registry --tests

echo "== Registry Client Step3 closure invariants =="
facade_loc="$(wc -l < "${facade_file}" | tr -d ' ')"
if [ "${facade_loc}" -gt 5 ]; then
  echo "facade grew beyond closure target: ${facade_loc} > 5"
  exit 1
fi

check_has_match '^\#\[path = "registry_client/mod.rs"\]$' "${facade_file}"
check_has_match '^mod registry_client;$' "${facade_file}"
check_no_match '^\#\[tokio::test\]' "${facade_file}"
check_no_match '^async fn test_' "${facade_file}"
check_no_match '^fn test_' "${facade_file}"

check_has_match '^mod scenarios_pack_fetch;$' "${mod_file}"
check_has_match '^mod scenarios_meta_keys;$' "${mod_file}"
check_has_match '^mod scenarios_auth_headers;$' "${mod_file}"
check_has_match '^mod scenarios_signature;$' "${mod_file}"
check_has_match '^mod scenarios_cache_digest;$' "${mod_file}"
check_has_match '^mod scenarios_retry;$' "${mod_file}"
check_has_match '^mod support;$' "${mod_file}"
check_no_match '^async fn test_' "${mod_file}"
check_no_match '^fn test_' "${mod_file}"

check_has_match '^pub\(super\) async fn create_test_client' "${support_file}"
check_no_match '^async fn test_' "${support_file}"

test_count="$("${rg_bin}" -n '^async fn test_' "${module_root}"/scenarios_*.rs | wc -l | tr -d ' ')"
echo "test inventory count: ${test_count}"
if [ "${test_count}" -ne 26 ]; then
  echo "test inventory drift: expected 26, got ${test_count}"
  exit 1
fi

echo "== Registry Client Step3 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-registry/tests/registry_client.rs$|^crates/assay-registry/tests/registry_client/[^/]+\.rs$|^docs/contributing/SPLIT-CHECKLIST-registry-client-step3.md$|^docs/contributing/SPLIT-REVIEW-PACK-registry-client-step3.md$|^scripts/ci/review-registry-client-step3.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in registry-client Step3"
  exit 1
fi

echo "Registry Client Step3 reviewer script: PASS"
