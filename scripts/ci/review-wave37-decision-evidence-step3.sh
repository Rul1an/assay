#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-wave37-decision-evidence-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave37-decision-evidence-step3.md"
  "scripts/ci/review-wave37-decision-evidence-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave37 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave37 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] no untracked files under bounded runtime scope"
for p in \
  "crates/assay-core/src/mcp" \
  "crates/assay-core/tests" \
  "crates/assay-cli/src/cli/commands" \
  "crates/assay-mcp-server"
do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] rerun Step2 invariants"

echo "[review] convergence markers"
for marker in \
  'DecisionOutcomeKind' \
  'DecisionOrigin' \
  'OutcomeCompatState' \
  'decision_outcome_kind' \
  'decision_origin' \
  'outcome_compat_state' \
  'PolicyDeny' \
  'FailClosedDeny' \
  'EnforcementDeny' \
  'ObligationApplied' \
  'ObligationSkipped' \
  'ObligationError' \
  'classify_decision_outcome'
do
  rg -n "$marker" crates/assay-core/src/mcp/decision.rs crates/assay-core/src/mcp/decision/outcome_convergence.rs >/dev/null || {
    echo "FAIL: missing convergence marker: $marker"
    exit 1
  }
done

echo "[review] existing normalization markers remain present"
for marker in \
  'fulfillment_decision_path' \
  'obligation_applied_present' \
  'obligation_skipped_present' \
  'obligation_error_present' \
  'reason_code' \
  'enforcement_stage' \
  'normalization_version'
do
  rg -n "$marker" crates/assay-core/src/mcp/decision.rs >/dev/null || {
    echo "FAIL: missing existing normalization marker: $marker"
    exit 1
  }
done

echo "[review] policy deny vs fail-closed deny separation"
rg -n 'classify_fulfillment_decision_path|fail_closed_applied' crates/assay-core/src/mcp/decision.rs >/dev/null || {
  echo "FAIL: missing deny-path separation markers"
  exit 1
}

echo "[review] existing obligation line remains present"
rg -n 'obligation_outcomes|legacy_warning|approval_required|restrict_scope|redact_args|log|alert' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: existing obligation markers missing"
  exit 1
}

echo "[review] no scope creep into non-goals"
if rg -n 'policy backend replacement|approval UI|case management|external approval|control-plane|auth transport' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected in implementation scope"
  rg -n 'policy backend replacement|approval UI|case management|external approval|control-plane|auth transport' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core --test decision_emit_invariant
cargo test -p assay-core --test fulfillment_normalization
cargo test -p assay-core mcp::tool_call_handler::tests::test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core --test decision_emit_invariant test_alert_obligation_outcome_emitted -- --exact
cargo test -p assay-core approval_required_missing_denies -- --exact
cargo test -p assay-core approval_required_expired_denies -- --exact
cargo test -p assay-core approval_required_bound_tool_mismatch_denies -- --exact
cargo test -p assay-core approval_required_bound_resource_mismatch_denies -- --exact
cargo test -p assay-core restrict_scope_mismatch_denies -- --exact
cargo test -p assay-core restrict_scope_match_sets_additive_fields -- --exact
cargo test -p assay-core redact_args_target_missing_denies -- --exact
cargo test -p assay-core redact_args_apply_failed_denies -- --exact
cargo test -p assay-core fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -p assay-core fulfillment_sets_policy_deny_convergence_fields -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server --test auth_integration

echo "[review] PASS"
