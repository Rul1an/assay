#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  # core MCP runtime
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/policy.rs"
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/policy/engine.rs"
  "crates/assay-core/src/mcp/policy/legacy.rs"
  "crates/assay-core/src/mcp/policy/schema.rs"
  "crates/assay-core/src/mcp/policy/response.rs"
  "crates/assay-core/src/mcp/proxy.rs"
  "crates/assay-core/src/mcp/obligations.rs"
  "crates/assay-core/src/mcp/tool_call_handler.rs"
  "crates/assay-core/src/mcp/tool_call_handler/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/evaluate.rs"
  "crates/assay-core/src/mcp/tool_call_handler/emit.rs"
  "crates/assay-core/src/mcp/tool_call_handler/types.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests.rs"

  # core tests
  "crates/assay-core/tests/decision_emit_invariant.rs"
  "crates/assay-core/tests/tool_taxonomy_policy_match.rs"

  # CLI compat consumers, only if needed
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/session_state_window.rs"
  "crates/assay-cli/src/cli/commands/coverage.rs"
  "crates/assay-cli/src/cli/commands/coverage/mod.rs"
  "crates/assay-cli/src/cli/commands/coverage/generate.rs"
  "crates/assay-cli/src/cli/commands/coverage/legacy.rs"
  "crates/assay-cli/src/cli/commands/coverage/io.rs"
  "crates/assay-cli/src/cli/commands/coverage/report.rs"
  "crates/assay-cli/src/cli/commands/coverage/schema.rs"
  "crates/assay-cli/src/cli/commands/coverage/format_md.rs"

  # MCP server, only if needed
  "crates/assay-mcp-server/src/auth.rs"
  "crates/assay-mcp-server/tests/auth_integration.rs"

  # docs if marker sync is needed
  "docs/architecture/ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md"
  "docs/architecture/PLAN-ADR-032-MCP-POLICY-ENFORCEMENT-2026q2.md"

  # step2 docs
  "docs/contributing/SPLIT-CHECKLIST-wave28-approval-required-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave28-approval-required-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave28-approval-required-step2.md"

  # gate
  "scripts/ci/review-wave28-approval-required-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave28 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  # allow bounded expansion inside MCP runtime dirs only
  if [[ "$ok" != "true" && "$f" == crates/assay-core/src/mcp/* ]]; then
    ok="true"
  fi
  if [[ "$ok" != "true" && "$f" == crates/assay-core/tests/* ]]; then
    ok="true"
  fi

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave28 Step2: $f"
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

echo "[review] approval artifact/evidence markers"
rg -n 'approval_id|approver|issued_at|expires_at|scope|approval_bound_tool|approval_bound_resource|approval_freshness|approval_state' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing approval artifact/evidence markers"
  exit 1
}

echo "[review] approval_required enforcement markers"
rg -n 'approval_required' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing approval_required marker"
  exit 1
}

rg -n 'missing approval|expired approval|bound tool mismatch|bound resource mismatch|approval_failure_reason' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing approval failure markers/reasons"
  exit 1
}

echo "[review] deny outcome markers for approval failures"
rg -n 'deny|Deny' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing deny markers"
  exit 1
}

echo "[review] no scope creep into non-goals"
if rg -n 'approval UI|case management|external approval|restrict_scope|redact_args|grace period|approval renewal|broad/global approval' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected in implementation scope"
  rg -n 'approval UI|case management|external approval|restrict_scope|redact_args|grace period|approval renewal|broad/global approval' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] existing obligation execution remains present"
rg -n 'obligation_outcomes|legacy_warning|log|alert' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: existing obligation execution markers missing"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-core test_allow_with_warning_emits_log_obligation_outcome -- --exact
cargo test -p assay-core test_tool_drift_deny_emits_alert_obligation_outcome -- --exact
# Add/replace these with exact approval enforcement tests in the implementation:
cargo test -p assay-core approval_required_missing_denies
cargo test -p assay-core approval_required_expired_denies
cargo test -p assay-core approval_required_bound_tool_mismatch_denies
cargo test -p assay-core approval_required_bound_resource_mismatch_denies
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
