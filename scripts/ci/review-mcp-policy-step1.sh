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

echo '== MCP Policy Step1 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core test_mixed_tools_config -- --exact

echo '== MCP Policy Step1 freeze checks =='
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-core/src/mcp/'; then
  echo 'tracked changes under crates/assay-core/src/mcp/** are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- crates/assay-core/src/mcp | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/mcp/** are forbidden in Step1'
  exit 1
fi

echo '== MCP Policy Step1 diff allowlist =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-PLAN-wave15-mcp-policy.md$|^docs/contributing/SPLIT-CHECKLIST-mcp-policy-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step1.md$|^scripts/ci/review-mcp-policy-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in mcp-policy Step1'
  exit 1
fi

echo 'MCP Policy Step1 reviewer script: PASS'
