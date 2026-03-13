#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave35-obligation-fulfillment-normalization.md"
  "docs/contributing/SPLIT-CHECKLIST-wave35-fulfillment-normalization-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave35-fulfillment-normalization-step1.md"
  "scripts/ci/review-wave35-fulfillment-normalization-step1.sh"
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
    echo "FAIL: Wave35 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave35 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave35 Step1 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave35-obligation-fulfillment-normalization.md"

rg -n '^# SPLIT PLAN — Wave35 Obligation Fulfillment Normalization$' "$PLAN" >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

for marker in \
  'obligation_outcomes' \
  'obligation_type' \
  'status' \
  'reason_code' \
  'enforcement_stage' \
  'normalization_version' \
  'policy_deny' \
  'fail_closed_deny' \
  'obligation_skipped' \
  'obligation_applied' \
  'obligation_error'
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
cargo test -p assay-core mcp::tool_call_handler::tests::test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core --test decision_emit_invariant test_alert_obligation_outcome_emitted -- --exact
cargo test -p assay-core approval_required_missing_denies -- --exact
cargo test -p assay-core restrict_scope_mismatch_denies -- --exact
cargo test -p assay-core redact_args_target_missing_denies -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
