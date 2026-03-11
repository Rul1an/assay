#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/emit.rs"
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/proxy.rs"
  "crates/assay-core/tests/decision_emit_invariant.rs"
  "docs/contributing/SPLIT-CHECKLIST-wave27-approval-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave27-approval-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step2.md"
  "scripts/ci/review-wave27-approval-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave27 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave27 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
for marker in \
  'approval_id' \
  'approver' \
  'issued_at' \
  'expires_at' \
  'scope' \
  'bound_tool' \
  'bound_resource' \
  'approval_state' \
  'approval_freshness'
do
  rg -n "$marker" crates/assay-core/src/mcp >/dev/null || {
    echo "FAIL: missing marker in MCP runtime: $marker"
    exit 1
  }
done

echo "[review] no approval enforcement in this wave"
if rg -n 'approval_required|enforce_approval|required_approval' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server >/dev/null; then
  echo "FAIL: approval enforcement markers detected in implementation scope"
  rg -n 'approval_required|enforce_approval|required_approval' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-core test_with_policy_context_sets_approval_artifact_fields -- --exact
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_tool_drift_deny_emits_alert_obligation_outcome -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
