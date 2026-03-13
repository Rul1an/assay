#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-wave33-obligation-outcomes-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave33-obligation-outcomes-step3.md"
  "scripts/ci/review-wave33-obligation-outcomes-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave33 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave33 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] rerun Step2 invariants"

echo "[review] normalization field markers"
for marker in \
  'reason_code' \
  'enforcement_stage' \
  'normalization_version' \
  'obligation_type' \
  'status' \
  'reason'
do
  rg -n "$marker" crates/assay-core/src/mcp/decision.rs >/dev/null || {
    echo "FAIL: missing outcome field marker: $marker"
    exit 1
  }
done

echo "[review] reason-code baseline markers"
for marker in \
  'legacy_warning_mapped' \
  'validated_in_handler' \
  'contract_only' \
  'unsupported_obligation_type' \
  'approval_missing' \
  'approval_expired' \
  'approval_bound_tool_mismatch' \
  'approval_bound_resource_mismatch' \
  'scope_target_missing' \
  'scope_target_mismatch' \
  'scope_match_mode_unsupported' \
  'scope_type_unsupported' \
  'redaction_target_missing' \
  'redaction_mode_unsupported' \
  'redaction_scope_unsupported' \
  'redaction_apply_failed'
do
  rg -n "$marker" crates/assay-core/src/mcp crates/assay-core/src/mcp/tool_call_handler/tests.rs >/dev/null || {
    echo "FAIL: missing reason-code marker: $marker"
    exit 1
  }
done

echo "[review] no scope creep into new behavior"
if rg -n 'execute_obligation|new_obligation_type|workflow_integration|control_plane' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected"
  rg -n 'execute_obligation|new_obligation_type|workflow_integration|control_plane' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_tool_drift_deny_emits_alert_obligation_outcome -- --exact
cargo test -p assay-core approval_required_missing_denies
cargo test -p assay-core approval_required_expired_denies
cargo test -p assay-core approval_required_bound_tool_mismatch_denies
cargo test -p assay-core approval_required_bound_resource_mismatch_denies
cargo test -p assay-core restrict_scope_mismatch_denies
cargo test -p assay-core restrict_scope_match_sets_additive_fields
cargo test -p assay-core redact_args_contract_sets_additive_fields
cargo test -p assay-core redact_args_target_missing_denies
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
