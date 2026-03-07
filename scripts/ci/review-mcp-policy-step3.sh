#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/codex/wave15-mcp-policy-step2-mechanical"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

echo '== MCP Policy Step3 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-mcp-policy-step3\.md$|^docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step3\.md$|^scripts/ci/review-mcp-policy-step3\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in MCP Policy Step3'
  exit 1
fi

if git status --porcelain -- crates/assay-core/src/mcp/policy | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/mcp/policy/** are forbidden in Step3'
  exit 1
fi

echo '== MCP Policy Step3 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core test_mixed_tools_config -- --exact

echo '== MCP Policy Step3 facade invariants =='
facade='crates/assay-core/src/mcp/policy/mod.rs'
facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 320 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 320): ${facade}"
  exit 1
fi

check_count() {
  local pattern="$1"
  local expected="$2"
  local count
  count="$("${rg_bin}" -n "${pattern}" "${facade}" | "${rg_bin}" -v '^\s*//' | wc -l | tr -d ' ')"
  if [ "${count}" -ne "${expected}" ]; then
    echo "expected ${expected} non-comment hits for pattern '${pattern}', got ${count}"
    exit 1
  fi
}

check_count 'legacy::from_file\(' 1
check_count 'legacy::validate\(' 1
check_count 'legacy::is_v1_format\(' 1
check_count 'legacy::normalize_legacy_shapes\(' 1
check_count 'schema::migrate_constraints_to_schemas\(' 1
check_count 'schema::compile_all_schemas\(' 2
check_count 'engine::evaluate_with_metadata\(' 1
check_count 'engine::check\(' 1

if "${rg_bin}" -n '^\s*mod tests\s*\{' "${facade}" >/dev/null; then
  echo 'inline mod tests { ... } is forbidden in policy facade'
  exit 1
fi

if "${rg_bin}" -n 'E_TOOL_|E_ARG_SCHEMA|E_RATE_LIMIT|E_TOOL_UNCONSTRAINED' "${facade}" >/dev/null; then
  echo 'decision logic markers leaked into facade mod.rs'
  "${rg_bin}" -n 'E_TOOL_|E_ARG_SCHEMA|E_RATE_LIMIT|E_TOOL_UNCONSTRAINED' "${facade}"
  exit 1
fi

"${rg_bin}" -n '^pub use response::make_deny_response;' "${facade}" >/dev/null

echo '== MCP Policy Step3 visibility invariants =='
internal_pub="$({ "${rg_bin}" -n '^\s*pub(\s|\()' crates/assay-core/src/mcp/policy/engine.rs crates/assay-core/src/mcp/policy/schema.rs crates/assay-core/src/mcp/policy/legacy.rs | "${rg_bin}" -v 'pub\(super\)' || true; })"
if [ -n "${internal_pub}" ]; then
  echo 'engine/schema/legacy must only expose pub(super) items:'
  echo "${internal_pub}"
  exit 1
fi

"${rg_bin}" -n '^pub fn make_deny_response\(' crates/assay-core/src/mcp/policy/response.rs >/dev/null

echo 'MCP Policy Step3 reviewer script: PASS'
