#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step3.md"
  "scripts/ci/review-wave29-restrict-scope-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave29 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave29 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] rerun Step2 invariants"

echo "[review] marker checks: restrict_scope shape + evidence"
for marker in \
  'restrict_scope' \
  'scope_type' \
  'scope_value' \
  'scope_match_mode' \
  'scope_evaluation_state' \
  'scope_failure_reason' \
  'restrict_scope_present' \
  'restrict_scope_target' \
  'restrict_scope_match' \
  'restrict_scope_reason'
do
  rg -n "$marker" crates/assay-core/src/mcp crates/assay-core/tests >/dev/null || {
    echo "FAIL: missing marker in runtime/tests: $marker"
    exit 1
  }
done

echo "[review] no restrict_scope execution/enforcement in this wave"
if rg -n \
  'P_RESTRICT_SCOPE|enforce_restrict_scope|validate_restrict_scope|restrict_scope_required|restrict_scope.*emit_deny|emit_deny.*restrict_scope|restrict_scope.*PolicyDecision::Deny|rewrite_args|filter_args|redact_args' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server >/dev/null; then
  echo "FAIL: restrict_scope enforcement/rewriting markers detected"
  rg -n \
    'P_RESTRICT_SCOPE|enforce_restrict_scope|validate_restrict_scope|restrict_scope_required|restrict_scope.*emit_deny|emit_deny.*restrict_scope|restrict_scope.*PolicyDecision::Deny|rewrite_args|filter_args|redact_args' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server
  exit 1
fi

echo "[review] existing obligation line remains present"
rg -n 'obligation_outcomes|legacy_warning|approval_required|log|alert' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: existing obligation markers missing"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_tool_drift_deny_emits_alert_obligation_outcome -- --exact
cargo test -p assay-core approval_required_missing_denies
cargo test -p assay-core approval_required_expired_denies
cargo test -p assay-core approval_required_bound_tool_mismatch_denies
cargo test -p assay-core approval_required_bound_resource_mismatch_denies
cargo test -p assay-core restrict_scope_mismatch_does_not_deny
cargo test -p assay-core restrict_scope_match_sets_additive_fields
cargo test -p assay-core execute_log_only_marks_restrict_scope_as_contract_only
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
