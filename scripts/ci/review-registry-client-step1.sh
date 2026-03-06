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
target_file="crates/assay-registry/tests/registry_client.rs"

echo "== Registry Client Step1 quality checks =="
cargo fmt --check
cargo test -p assay-registry --tests

echo "== Registry Client Step1 freeze gate =="
if ! git diff --quiet "${base_ref}...HEAD" -- "${target_file}"; then
  echo "${target_file} changed in Step1; freeze step must not edit target test file"
  git diff -- "${target_file}" | sed -n '1,200p'
  exit 1
fi

echo "== Registry Client Step1 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-PLAN-registry-client-wave11.md$|^docs/contributing/SPLIT-CHECKLIST-registry-client-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-registry-client-step1.md$|^scripts/ci/review-registry-client-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in registry-client Step1"
  exit 1
fi

echo "Registry Client Step1 reviewer script: PASS"
