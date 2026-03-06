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

echo "== Registry Client Step2 quality checks =="
cargo fmt --check
cargo clippy -p assay-registry --tests -- -D warnings
cargo test -p assay-registry --tests

echo "== Registry Client Step2 mechanical split gates =="
if ! "${rg_bin}" -n '^\#\[path = "registry_client/mod.rs"\]$' "${facade_file}" >/dev/null; then
  echo "missing path module wiring in ${facade_file}"
  exit 1
fi
if ! "${rg_bin}" -n '^mod registry_client;$' "${facade_file}" >/dev/null; then
  echo "missing module declaration in ${facade_file}"
  exit 1
fi
if "${rg_bin}" -n '^\#\[tokio::test\]' "${facade_file}"; then
  echo "inline test functions remain in ${facade_file}"
  exit 1
fi
if ! "${rg_bin}" -n '^pub\(super\) async fn create_test_client' "${module_root}/support.rs" >/dev/null; then
  echo "shared helper missing: create_test_client"
  exit 1
fi

old_count="$(
  git show "${base_ref}:${facade_file}" | "${rg_bin}" -n '^async fn test_' | wc -l | tr -d ' '
)"
new_count="$(
  "${rg_bin}" -n '^async fn test_' "${module_root}"/*.rs | wc -l | tr -d ' '
)"
echo "test inventory count: before=${old_count} after=${new_count}"
if [ "${new_count}" -ne "${old_count}" ]; then
  echo "test inventory drift detected"
  exit 1
fi

echo "== Registry Client Step2 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-registry/tests/registry_client.rs$|^crates/assay-registry/tests/registry_client/[^/]+\.rs$|^docs/contributing/SPLIT-CHECKLIST-registry-client-step2.md$|^docs/contributing/SPLIT-MOVE-MAP-registry-client-step2.md$|^docs/contributing/SPLIT-REVIEW-PACK-registry-client-step2.md$|^scripts/ci/review-registry-client-step2.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in registry-client Step2"
  exit 1
fi

echo "Registry Client Step2 reviewer script: PASS"
