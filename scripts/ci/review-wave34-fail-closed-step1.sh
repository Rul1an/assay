#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave34-fail-closed-matrix-typing.md"
  "docs/contributing/SPLIT-CHECKLIST-wave34-fail-closed-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave34-fail-closed-step1.md"
  "scripts/ci/review-wave34-fail-closed-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp"
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/coverage"
  "crates/assay-cli/src/cli/commands/session_state_window.rs"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave34 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave34 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave34 Step1 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for p in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
rg -n '^# SPLIT PLAN - Wave34 Fail-Closed Matrix Typing$' \
  docs/contributing/SPLIT-PLAN-wave34-fail-closed-matrix-typing.md >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

for marker in \
  'tool_risk_class' \
  'fail_closed_mode' \
  'fail_closed_trigger' \
  'fail_closed_applied' \
  'fail_closed_error_code' \
  'high_risk' \
  'low_risk_read' \
  'default' \
  'fail_closed' \
  'degrade_read_only' \
  'fail_safe_allow' \
  'policy_engine_unavailable' \
  'context_provider_unavailable' \
  'runtime_dependency_error' \
  'fail_closed_policy_engine_unavailable' \
  'fail_closed_context_provider_unavailable' \
  'fail_closed_runtime_dependency_error' \
  'degrade_read_only_runtime_dependency_error'
do
  rg -n "$marker" docs/contributing/SPLIT-PLAN-wave34-fail-closed-matrix-typing.md >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core --test decision_emit_invariant
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_alert_obligation_outcome_emitted -- --exact
cargo test -p assay-core approval_required_missing_denies
cargo test -p assay-core approval_required_expired_denies
cargo test -p assay-core approval_required_bound_tool_mismatch_denies
cargo test -p assay-core approval_required_bound_resource_mismatch_denies
cargo test -p assay-core restrict_scope_mismatch_denies
cargo test -p assay-core restrict_scope_match_sets_additive_fields
cargo test -p assay-core redact_args_target_missing_denies
cargo test -p assay-core redact_args_apply_failed_denies
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
