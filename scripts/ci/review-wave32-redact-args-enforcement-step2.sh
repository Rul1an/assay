#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/policy/engine.rs"
  "crates/assay-core/src/mcp/tool_call_handler/emit.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate.rs"
  "crates/assay-core/src/mcp/proxy.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests.rs"
  "crates/assay-core/tests/decision_emit_invariant.rs"
  "docs/contributing/SPLIT-CHECKLIST-wave32-redact-args-enforcement-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave32-redact-args-enforcement-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave32-redact-args-enforcement-step2.md"
  "scripts/ci/review-wave32-redact-args-enforcement-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave32 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave32 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] no untracked files under bounded scope"
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

echo "[review] enforcement markers"
for marker in \
  'P_REDACT_ARGS' \
  'validate_redact_args' \
  'redaction_target_missing' \
  'redaction_mode_unsupported' \
  'redaction_scope_unsupported' \
  'redaction_apply_failed'
do
  rg -n "$marker" crates/assay-core/src/mcp crates/assay-core/tests >/dev/null || {
    echo "FAIL: missing enforcement marker: $marker"
    exit 1
  }
done

rg -n 'emit_deny\(reason_codes::P_REDACT_ARGS' crates/assay-core/src/mcp/tool_call_handler/evaluate.rs >/dev/null || {
  echo "FAIL: missing deny emission for P_REDACT_ARGS"
  exit 1
}

echo "[review] additive evidence markers remain present"
for marker in \
  'redaction_target' \
  'redaction_mode' \
  'redaction_scope' \
  'redaction_applied_state' \
  'redaction_reason' \
  'redaction_failure_reason' \
  'redact_args_present' \
  'redact_args_target' \
  'redact_args_mode' \
  'redact_args_result' \
  'redact_args_reason'
do
  rg -n "$marker" crates/assay-core/src/mcp crates/assay-core/tests >/dev/null || {
    echo "FAIL: missing redaction evidence marker: $marker"
    exit 1
  }
done

echo "[review] no scope creep into non-goals"
if rg -n 'global_redact|pii_detection|external_dlp|dlp_integration|redaction_inheritance|control_plane' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected"
  rg -n 'global_redact|pii_detection|external_dlp|dlp_integration|redaction_inheritance|control_plane' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] existing obligation line remains present"
for marker in \
  'obligation_outcomes' \
  'legacy_warning' \
  'log' \
  'alert' \
  'approval_required' \
  'restrict_scope'
do
  rg -n "$marker" crates/assay-core/src/mcp >/dev/null || {
    echo "FAIL: missing existing obligation marker: $marker"
    exit 1
  }
done

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
cargo test -p assay-core restrict_scope_mismatch_denies
cargo test -p assay-core restrict_scope_target_missing_denies
cargo test -p assay-core restrict_scope_unsupported_match_mode_denies
cargo test -p assay-core restrict_scope_unsupported_scope_type_denies
cargo test -p assay-core restrict_scope_match_sets_additive_fields
cargo test -p assay-core redact_args_contract_sets_additive_fields
cargo test -p assay-core redact_args_target_missing_denies
cargo test -p assay-core redact_args_mode_unsupported_denies
cargo test -p assay-core redact_args_scope_unsupported_denies
cargo test -p assay-core redact_args_apply_failed_denies
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
