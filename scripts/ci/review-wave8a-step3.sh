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

echo "== Wave8A Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-adapter-a2a -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-a2a
bash scripts/ci/test-adapter-a2a.sh

echo "== Wave8A Step3 invariant checks =="
test -f crates/assay-adapter-a2a/src/lib.rs || { echo "missing a2a facade"; exit 1; }
test -d crates/assay-adapter-a2a/src/adapter_impl || { echo "missing adapter_impl dir"; exit 1; }
test -f crates/assay-adapter-a2a/src/adapter_impl/convert.rs || { echo "missing convert.rs"; exit 1; }
test -f crates/assay-adapter-a2a/src/adapter_impl/tests.rs || { echo "missing tests.rs"; exit 1; }

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

check_has_match '^mod adapter_impl;$' 'crates/assay-adapter-a2a/src/lib.rs'
check_no_match '^fn parse_packet\(|^fn validate_protocol\(|^fn observed_version\(|^fn build_payload\(' 'crates/assay-adapter-a2a/src/lib.rs'
check_has_match 'pub struct A2aAdapter' 'crates/assay-adapter-a2a/src/lib.rs'
check_has_match 'impl ProtocolAdapter for A2aAdapter' 'crates/assay-adapter-a2a/src/lib.rs'

echo "== Wave8A Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v '^docs/contributing/SPLIT-CHECKLIST-wave8a-step1-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-a2a.md$|^scripts/ci/review-wave8a-step1.sh$|^crates/assay-adapter-a2a/src/lib.rs$|^crates/assay-adapter-a2a/src/adapter_impl/|^docs/contributing/SPLIT-MOVE-MAP-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step2-a2a.md$|^scripts/ci/review-wave8a-step2.sh$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step3-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step3-a2a.md$|^scripts/ci/review-wave8a-step3.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "$rg_bin" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave8A Step3"
  exit 1
fi

echo "Wave8A Step3 reviewer script: PASS"
