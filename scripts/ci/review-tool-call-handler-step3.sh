#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/codex/wave16-tool-call-handler-step2-mechanical"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi

echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"

echo '== Tool call handler Step3 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step3\.md$|^docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step3\.md$|^scripts/ci/review-tool-call-handler-step3\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in tool-call-handler Step3'
  exit 1
fi

if git status --porcelain -- crates/assay-core/src/mcp/tool_call_handler | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/mcp/tool_call_handler/** are forbidden in Step3'
  exit 1
fi

echo '== Tool call handler Step3 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact

echo '== Tool call handler Step3 facade invariants =='
facade='crates/assay-core/src/mcp/tool_call_handler/mod.rs'
facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 220 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 220): ${facade}"
  exit 1
fi

"${rg_bin}" -n '^mod emit;\s*$' "${facade}" >/dev/null || { echo "missing 'mod emit;'"; exit 1; }
"${rg_bin}" -n '^mod evaluate;\s*$' "${facade}" >/dev/null || { echo "missing 'mod evaluate;'"; exit 1; }
"${rg_bin}" -n '^mod types;\s*$' "${facade}" >/dev/null || { echo "missing 'mod types;'"; exit 1; }
"${rg_bin}" -n '^pub use types::\{HandleResult, ToolCallHandler, ToolCallHandlerConfig\};\s*$' "${facade}" >/dev/null || {
  echo 'missing public surface re-export from types'
  exit 1
}

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

check_count 'types::new_handler\(' 1
check_count 'types::with_lifecycle_emitter\(' 1
check_count 'evaluate::handle_tool_call\(' 1

if "${rg_bin}" -n 'DecisionEvent::new\(' "${facade}" >/dev/null; then
  echo 'DecisionEvent::new(...) must not appear in facade'
  exit 1
fi

if "${rg_bin}" -n '^\s*#\[test\]' "${facade}" >/dev/null; then
  echo 'tests must not remain in facade'
  exit 1
fi

echo '== Tool call handler Step3 module boundary invariants =='
non_emit_decision_new="$({ "${rg_bin}" -n 'DecisionEvent::new\(' crates/assay-core/src/mcp/tool_call_handler/*.rs | "${rg_bin}" -v 'emit\.rs' || true; })"
if [ -n "${non_emit_decision_new}" ]; then
  echo 'DecisionEvent::new(...) is only allowed in emit.rs:'
  echo "${non_emit_decision_new}"
  exit 1
fi

if "${rg_bin}" -n 'PolicyDecision|evaluate_with_metadata|authorize_and_consume' crates/assay-core/src/mcp/tool_call_handler/types.rs >/dev/null; then
  echo 'types.rs must not contain evaluation/authorization behavior logic'
  exit 1
fi

echo '== Tool call handler Step3 test relocation invariants =='
"${rg_bin}" -n '^\s*fn test_handler_emits_decision_on_policy_deny\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_handler_emits_decision_on_policy_allow\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_commit_tool_without_mandate_denied\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_is_commit_tool_matching\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_operation_class_for_tool\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_lifecycle_emitter_not_called_when_none\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null

echo 'Tool call handler Step3 reviewer script: PASS'
