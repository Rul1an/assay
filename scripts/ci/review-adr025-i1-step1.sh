#!/usr/bin/env bash
set -euo pipefail

# ADR-025 I1 Step1 freeze reviewer script
# Default mode is sandbox-friendly:
# - fmt + clippy always
# - tests skip assay-mcp-server integration binding constraints unless ASSAY_FULL_TESTS=1

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

echo "== ADR-025 I1 Step1 scope allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^docs/architecture/PLAN-ADR-025-I1-audit-kit-soak-2026q2\.md$|^schemas/soak_report_v1\.schema\.json$|^scripts/ci/review-adr025-i1-step1\.sh$' || true
)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

echo "== ADR-025 I1 Step1 workflow safety tripwire =="
wf_touched="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" '^\.github/workflows/' || true
)"
if [ -n "${wf_touched}" ]; then
  echo "workflow changes are out of scope for Step1:"
  echo "${wf_touched}"
  exit 1
fi

echo "== ADR-025 I1 Step1 schema checks =="
jq . schemas/soak_report_v1.schema.json >/dev/null
schema_uri="$(jq -r '."$schema"' schemas/soak_report_v1.schema.json)"
if [ "${schema_uri}" != "https://json-schema.org/draft/2020-12/schema" ]; then
  echo "unexpected \$schema URI: ${schema_uri}"
  exit 1
fi
jq -e '.properties.schema_version.const == "soak-report-v1"' schemas/soak_report_v1.schema.json >/dev/null
jq -e '.properties.report_version.const == 1' schemas/soak_report_v1.schema.json >/dev/null

echo "== ADR-025 I1 Step1 local-path hygiene =="
if "$rg_bin" -n "/Users/roelschuurkes/assay" \
  docs/architecture/PLAN-ADR-025-I1-audit-kit-soak-2026q2.md \
  schemas/soak_report_v1.schema.json >/dev/null; then
  echo "absolute local paths found in Step1 artifacts"
  exit 1
fi

echo "== ADR-025 I1 Step1 quality checks =="
cargo fmt --check
cargo clippy --workspace -- -D warnings

if [ "${ASSAY_FULL_TESTS:-0}" = "1" ]; then
  echo "Running full workspace tests (ASSAY_FULL_TESTS=1)"
  cargo test --workspace
else
  echo "Running sandbox-friendly test set (set ASSAY_FULL_TESTS=1 for full suite)"
  cargo test --workspace --exclude assay-mcp-server
  cargo test -p assay-mcp-server --lib
fi

echo "ADR-025 I1 Step1 reviewer script: PASS"
