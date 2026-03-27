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
  "docs/contributing/SPLIT-CHECKLIST-typed-decisions-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-typed-decisions-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step2.md"

  # gate
  "scripts/ci/review-wave24-typed-decisions-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave24 Step2 must not touch workflows ($f)"
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
    echo "FAIL: file not allowed in Wave24 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] no untracked files under MCP runtime scope"
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

echo "[review] typed decision markers"

for marker in allow_with_obligations deny_with_alert; do
  rg -n "$marker" crates/assay-core/src/mcp >/dev/null || {
    echo "FAIL: missing typed decision marker: $marker"
    exit 1
  }
done

rg -n 'AllowWithWarning' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing AllowWithWarning compatibility path"
  exit 1
}

echo "[review] Decision Event v2 field markers"
for field in policy_version policy_digest obligations approval_state lane principal auth_context_summary; do
  rg -n "$field" crates/assay-core/src/mcp >/dev/null || {
    echo "FAIL: missing Decision Event v2 field marker: $field"
    exit 1
  }
done

echo "[review] required legacy decision markers still present"
rg -n 'tool_classes|matched_tool_classes|match_basis|matched_rule|reason_code' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing existing decision-event fields"
  exit 1
}

echo "[review] no obligations execution in this wave"
if rg -n 'approval_required|redact_args|restrict_scope|obligation_fulfillment|execute_obligation' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: obligations execution markers detected in implementation scope"
  rg -n 'approval_required|redact_args|restrict_scope|obligation_fulfillment|execute_obligation' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
