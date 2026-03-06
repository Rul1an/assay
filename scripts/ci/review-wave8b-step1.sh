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

file="crates/assay-adapter-ucp/src/lib.rs"
rg_bin="$(command -v rg)"

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${file}" | "$rg_bin" -n "$pattern" || true; } | wc -l | tr -d ' ')"
  after="$({ "$rg_bin" -n "$pattern" "$file" || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave8B Step1 quality checks =="
cargo fmt --check
cargo clippy -p assay-adapter-ucp -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-ucp
bash scripts/ci/test-adapter-ucp.sh

echo "== Wave8B Step1 contract anchors =="
for test_name in \
  tests::protocol_metadata_uses_frozen_release_tag \
  tests::strict_order_fixture_maps_expected_event \
  tests::strict_missing_order_id_fails_with_measurement_error \
  tests::lenient_missing_order_id_substitutes_unknown_order \
  tests::excessive_json_depth_fails_measurement_contract

do
  echo "anchor: ${test_name}"
  cargo test -p assay-adapter-ucp "${test_name}" -- --exact
done

echo "== Wave8B Step1 no-production-change gate =="
if ! git diff --quiet "${base_ref}...HEAD" -- "${file}"; then
  echo "${file} changed in Step1; only docs/reviewer artifacts are allowed"
  git diff -- "${file}" | sed -n '1,160p'
  exit 1
fi

echo "== Wave8B Step1 drift gates =="
check_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
check_no_increase '\bunsafe\b' 'unsafe'
check_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
check_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave8B Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v '^crates/assay-adapter-a2a/src/lib.rs$|^crates/assay-adapter-a2a/src/adapter_impl/|^docs/contributing/SPLIT-CHECKLIST-wave8a-step1-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-a2a.md$|^scripts/ci/review-wave8a-step1.sh$|^docs/contributing/SPLIT-MOVE-MAP-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step2-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step2-a2a.md$|^scripts/ci/review-wave8a-step2.sh$|^docs/contributing/SPLIT-CHECKLIST-wave8a-step3-a2a.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8a-step3-a2a.md$|^scripts/ci/review-wave8a-step3.sh$|^docs/contributing/SPLIT-CHECKLIST-wave8b-step1-ucp.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave8b-step1-ucp.md$|^scripts/ci/review-wave8b-step1.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "$rg_bin" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave8B Step1"
  exit 1
fi

echo "Wave8B Step1 reviewer script: PASS"
