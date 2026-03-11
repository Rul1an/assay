# SPLIT-MOVE-MAP — Wave24 Step2 — Typed Decisions + Decision Event v2

## Goal
Implement the Wave24 contract upgrade with bounded scope and no execution-semantics expansion:
1. typed decision contract surface
2. Decision Event v2 enrichment
3. `AllowWithWarning` compatibility preservation

## Contract mapping

### Decision model
- Legacy logical surface:
  - `Allow`
  - `AllowWithWarning`
  - `Deny`
- Target logical surface:
  - `allow`
  - `allow_with_obligations`
  - `deny`
  - `deny_with_alert`

Compatibility mapping:
- `AllowWithWarning` remains parseable and usable
- internal mapping may target `allow_with_obligations`
- warning context remains available as obligation metadata and/or compatibility fields

### Decision event model
Decision Event v2 adds:
- `policy_version`
- `policy_digest`
- `obligations`
- `approval_state`
- `lane`
- `principal`
- `auth_context_summary`

Existing event shape must retain:
- `tool`
- `tool_classes`
- `matched_tool_classes`
- `match_basis`
- `matched_rule`
- `reason_code`

## File-level implementation map

### Core runtime/policy path
- `crates/assay-core/src/mcp/policy/**`
  - introduce typed decision output contract
  - preserve legacy compatibility path
- `crates/assay-core/src/mcp/decision.rs`
  - emit Decision Event v2 fields
- `crates/assay-core/src/mcp/tool_call_handler/**`
  - keep pre-execution decision/emit path aligned with new contract

### Core tests
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - preserve required event fields + v2 additions
- `crates/assay-core/tests/tool_taxonomy_policy_match.rs`
  - preserve class matching + decision-event compatibility invariants

### CLI compat consumers (if needed)
- `crates/assay-cli/src/cli/commands/mcp.rs`
- `crates/assay-cli/src/cli/commands/session_state_window.rs`
- `crates/assay-cli/src/cli/commands/coverage/**`

Intent:
- keep normalizers/reports functioning with additive event fields
- prevent regressions in coverage/state-window projections

### MCP server compat (if needed)
- `crates/assay-mcp-server/src/auth.rs`
- `crates/assay-mcp-server/tests/auth_integration.rs`

Intent:
- only touch if required for compile/test compatibility
- no transport-auth redesign in this wave

## Behavior parity notes
- Existing allow/deny behavior remains stable.
- Decision-event flow remains deterministic and replay-friendly.
- No obligation execution behavior is introduced in Step2.
- No approval enforcement path is introduced in Step2.
- No policy-backend architecture changes in Step2.

## Out-of-scope guardrails
- no `approval_required` runtime execution logic
- no `redact_args` / `restrict_scope` execution logic
- no obligation fulfillment state machine
- no lane control-plane additions
