#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave36-redact-args-enforcement.md"
  "docs/contributing/SPLIT-CHECKLIST-wave36-redact-args-enforcement-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave36-redact-args-enforcement-step1.md"
  "scripts/ci/review-wave36-redact-args-enforcement-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp"
  "crates/assay-core/tests"
  "crates/assay-cli/src/cli/commands"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave36 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave36 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave36 Step1 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave36-redact-args-enforcement.md"

rg -n '^# SPLIT PLAN — Wave36 Redact Args Enforcement Hardening$' "$PLAN" >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

for marker in \
  'redact_args' \
  'P_REDACT_ARGS' \
  'redaction_target_missing' \
  'redaction_mode_unsupported' \
  'redaction_scope_unsupported' \
  'redaction_apply_failed' \
  'reason_code' \
  'enforcement_stage' \
  'normalization_version' \
  'redaction_failure_reason' \
  'obligation_outcomes'
do
  rg -n "$marker" "$PLAN" >/dev/null || {
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
cargo test -p assay-core approval_required_missing_denies -- --exact
cargo test -p assay-core restrict_scope_mismatch_denies -- --exact
cargo test -p assay-core redact_args_target_missing_denies -- --exact
cargo test -p assay-core redact_args_mode_unsupported_denies -- --exact
cargo test -p assay-core redact_args_scope_unsupported_denies -- --exact
cargo test -p assay-core redact_args_apply_failed_denies -- --exact
cargo test -p assay-core fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
