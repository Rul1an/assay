#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave30-restrict-scope-enforcement.md"
  "docs/contributing/SPLIT-CHECKLIST-wave30-restrict-scope-enforcement-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave30-restrict-scope-enforcement-step1.md"
  "scripts/ci/review-wave30-restrict-scope-enforcement-step1.sh"
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
    echo "FAIL: Wave30 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave30 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave30 Step1 must not change frozen path: $p"
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
rg -n '^# SPLIT PLAN — Wave30 Restrict Scope Enforcement$' \
  docs/contributing/SPLIT-PLAN-wave30-restrict-scope-enforcement.md >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

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
  'restrict_scope_reason' \
  'scope_target_missing' \
  'scope_target_mismatch' \
  'scope_match_mode_unsupported' \
  'scope_type_unsupported' \
  'deny'
do
  rg -n "$marker" docs/contributing/SPLIT-PLAN-wave30-restrict-scope-enforcement.md >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
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
cargo test -p assay-core restrict_scope_mismatch_does_not_deny
cargo test -p assay-core restrict_scope_match_sets_additive_fields
cargo test -p assay-core execute_log_only_marks_restrict_scope_as_contract_only
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
