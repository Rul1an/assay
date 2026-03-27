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

echo '== Tool call handler Step2 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact

echo '== Tool call handler Step2 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-core/src/mcp/tool_call_handler\.rs$|^crates/assay-core/src/mcp/tool_call_handler/|^crates/assay-core/src/mcp/mod\.rs$|^docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step2\.md$|^docs/contributing/SPLIT-MOVE-MAP-tool-call-handler-step2\.md$|^docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step2\.md$|^scripts/ci/review-tool-call-handler-step2\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in tool-call-handler Step2'
  exit 1
fi

if git status --porcelain -- crates/assay-core/src/mcp/tool_call_handler | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/mcp/tool_call_handler/** are forbidden in Step2'
  exit 1
fi

echo '== Tool call handler Step2 facade invariants =='
facade='crates/assay-core/src/mcp/tool_call_handler/mod.rs'
if [ ! -f "${facade}" ]; then
  echo "missing facade file: ${facade}"
  exit 1
fi

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

new_calls="$("${rg_bin}" -n 'types::new_handler\(' "${facade}" | "${rg_bin}" -v '^\s*//' | wc -l | tr -d ' ')"
if [ "${new_calls}" -ne 1 ]; then
  echo "expected exactly one call-site to types::new_handler(...), got ${new_calls}"
  exit 1
fi

lifecycle_calls="$("${rg_bin}" -n 'types::with_lifecycle_emitter\(' "${facade}" | "${rg_bin}" -v '^\s*//' | wc -l | tr -d ' ')"
if [ "${lifecycle_calls}" -ne 1 ]; then
  echo "expected exactly one call-site to types::with_lifecycle_emitter(...), got ${lifecycle_calls}"
  exit 1
fi

handle_calls="$("${rg_bin}" -n 'evaluate::handle_tool_call\(' "${facade}" | "${rg_bin}" -v '^\s*//' | wc -l | tr -d ' ')"
if [ "${handle_calls}" -ne 1 ]; then
  echo "expected exactly one call-site to evaluate::handle_tool_call(...), got ${handle_calls}"
  exit 1
fi

if "${rg_bin}" -n 'DecisionEvent::new\(' "${facade}" >/dev/null; then
  echo 'DecisionEvent::new(...) must not appear in facade'
  exit 1
fi

if "${rg_bin}" -n '^\s*#\[test\]' "${facade}" >/dev/null; then
  echo 'tests must not remain in facade'
  exit 1
fi

echo '== Tool call handler Step2 module boundary invariants =='
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

echo '== Tool call handler Step2 test relocation invariants =='
"${rg_bin}" -n '^\s*fn test_handler_emits_decision_on_policy_deny\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_handler_emits_decision_on_policy_allow\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_commit_tool_without_mandate_denied\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_is_commit_tool_matching\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_operation_class_for_tool\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null
"${rg_bin}" -n '^\s*fn test_lifecycle_emitter_not_called_when_none\(' crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null

echo 'Tool call handler Step2 reviewer script: PASS'
