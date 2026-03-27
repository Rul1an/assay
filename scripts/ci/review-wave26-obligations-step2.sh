#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/obligations.rs"
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests.rs"
  "docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave26-obligations-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step2.md"
  "scripts/ci/review-wave26-obligations-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave26 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave26 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
for marker in alert log legacy_warning obligation_outcomes deny_with_alert; do
  rg -n "$marker" crates/assay-core/src/mcp >/dev/null || {
    echo "FAIL: missing marker: $marker"
    exit 1
  }
done

echo "[review] no high-risk obligation execution in this wave"
if rg -n 'approval_required|restrict_scope|redact_args' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'docs/' >/dev/null; then
  echo "FAIL: high-risk obligation execution markers detected"
  rg -n 'approval_required|restrict_scope|redact_args' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_tool_drift_deny_emits_alert_obligation_outcome -- --exact
cargo test -p assay-core execute_log_only_
cargo test -p assay-core typed_contract_maps_tool_drift_to_deny_with_alert_obligation -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
