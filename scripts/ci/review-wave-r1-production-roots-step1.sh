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
files=(
  "crates/assay-registry/src/trust.rs"
  "crates/assay-registry/src/resolver.rs"
  "crates/assay-registry/tests/resolver_production_roots.rs"
)

count_pattern_in_file() {
  local file="$1"
  local pattern="$2"
  if git cat-file -e "${base_ref}:${file}" 2>/dev/null; then
    git show "${base_ref}:${file}" | { "$rg_bin" -n "$pattern" || true; } | wc -l | tr -d ' '
  else
    echo "0"
  fi
}

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before=0
  local after=0
  local file
  for file in "${files[@]}"; do
    before=$((before + $(count_pattern_in_file "${file}" "${pattern}")))
    if [ -f "${file}" ]; then
      after=$((after + $({ "$rg_bin" -n "$pattern" "${file}" || true; } | wc -l | tr -d ' ')))
    fi
  done
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave R1 Step1 quality checks =="
cargo fmt --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo test -q -p assay-registry
git diff --check

echo "== Wave R1 Step1 contract anchors =="
cargo test -q -p assay-registry trust::tests::test_with_production_roots_loads_embedded_roots -- --exact
cargo test -q -p assay-registry trust::tests::test_parse_pinned_roots_json_rejects_empty_rootset -- --exact
cargo test -q -p assay-registry --test resolver_production_roots

echo "== Wave R1 Step1 boundary gates =="
if "$rg_bin" -n 'TrustStore::new\(\)' crates/assay-registry/src/resolver.rs; then
  echo "resolver.rs must not silently fall back to TrustStore::new()"
  exit 1
fi
if ! "$rg_bin" -n 'TrustStore::from_production_roots\(\)\?' crates/assay-registry/src/resolver.rs; then
  echo "resolver.rs must use TrustStore::from_production_roots() in the production path"
  exit 1
fi

echo "== Wave R1 Step1 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave R1 Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v '^crates/assay-registry/assets/production-trust-roots.json$|^crates/assay-registry/src/resolver.rs$|^crates/assay-registry/src/trust.rs$|^crates/assay-registry/tests/resolver_production_roots.rs$|^docs/contributing/SPLIT-INVENTORY-wave-r1-production-roots-step1.md$|^docs/contributing/SPLIT-CHECKLIST-wave-r1-production-roots-step1.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-r1-production-roots-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-r1-production-roots-step1.md$|^scripts/ci/review-wave-r1-production-roots-step1.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "$rg_bin" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave R1 Step1"
  exit 1
fi

echo "Wave R1 Step1 reviewer script: PASS"
