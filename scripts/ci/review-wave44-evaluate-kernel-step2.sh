#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

changed_files() {
  {
    git diff --name-only "$BASE_REF"...HEAD
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | awk 'NF' | sort -u
}

ALLOWLIST=(
  "crates/assay-core/src/mcp/tool_call_handler/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs"
  "docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step2.md"
  "scripts/ci/review-wave44-evaluate-kernel-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, tests ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave44 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave44 Step2: $f"
    exit 1
  fi
done < <(changed_files)

if changed_files | rg -n '^crates/assay-core/tests/' >/dev/null; then
  echo "FAIL: Wave44 Step2 must not change crates/assay-core/tests/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/tests/** are not allowed in Wave44 Step2"
  git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | sed 's/^/  - /'
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/tool_call_handler/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/src/mcp/tool_call_handler/** are not allowed in Wave44 Step2"
  git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/tool_call_handler/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] facade/module boundary checks"
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs ]] || {
  echo "FAIL: missing evaluate_next/mod.rs"
  exit 1
}
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs ]] || {
  echo "FAIL: missing evaluate_next/approval.rs"
  exit 1
}
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs ]] || {
  echo "FAIL: missing evaluate_next/scope.rs"
  exit 1
}
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs ]] || {
  echo "FAIL: missing evaluate_next/redaction.rs"
  exit 1
}
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs ]] || {
  echo "FAIL: missing evaluate_next/fail_closed.rs"
  exit 1
}
[[ -f crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs ]] || {
  echo "FAIL: missing evaluate_next/classification.rs"
  exit 1
}

rg -n '^mod evaluate_next;$' crates/assay-core/src/mcp/tool_call_handler/mod.rs >/dev/null || {
  echo "FAIL: mod.rs must wire evaluate_next"
  exit 1
}
rg -n '^pub\(super\) fn handle_tool_call\(' crates/assay-core/src/mcp/tool_call_handler/evaluate.rs >/dev/null || {
  echo "FAIL: evaluate.rs must keep handle_tool_call(...)"
  exit 1
}

for pattern in \
  '^enum ApprovalFailure' \
  '^enum RestrictScopeFailure' \
  '^enum RedactArgsFailure' \
  '^fn validate_approval_required' \
  '^fn validate_restrict_scope' \
  '^fn validate_redact_args' \
  '^fn requested_resource' \
  '^fn seed_fail_closed_context' \
  '^fn runtime_dependency_error_code' \
  '^fn mark_fail_closed' \
  '^impl ToolCallHandler'; do
  if rg -n "$pattern" crates/assay-core/src/mcp/tool_call_handler/evaluate.rs >/dev/null; then
    echo "FAIL: evaluate.rs still contains extracted helper pattern: $pattern"
    exit 1
  fi
done

non_emit_decision_new="$({ rg -n 'DecisionEvent::new\(' crates/assay-core/src/mcp/tool_call_handler/evaluate.rs crates/assay-core/src/mcp/tool_call_handler/evaluate_next/*.rs || true; })"
if [[ -n "${non_emit_decision_new}" ]]; then
  echo "FAIL: DecisionEvent::new(...) must stay outside evaluate split modules:"
  echo "${non_emit_decision_new}"
  exit 1
fi

evaluate_loc="$(wc -l < crates/assay-core/src/mcp/tool_call_handler/evaluate.rs | tr -d '[:space:]')"
if [[ "${evaluate_loc}" -gt 320 ]]; then
  echo "FAIL: evaluate.rs facade budget exceeded (${evaluate_loc} > 320)"
  exit 1
fi

echo "[review] gates"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_expired_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_unsupported_match_mode_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_unsupported_scope_type_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact

echo "[review] PASS"
