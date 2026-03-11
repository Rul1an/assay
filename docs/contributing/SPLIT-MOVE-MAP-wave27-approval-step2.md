# SPLIT-MOVE-MAP - Wave27 Step2 - Approval Artifact Shape

## Goal
Implement a bounded Step2 that adds approval artifact and evidence shape only:
1. approval artifact/data shape
2. freshness/expiry fields
3. binding fields (`bound_tool`, `bound_resource`)
4. additive approval evidence fields

No runtime enforcement is added in this step.

## File-level map

### Core runtime/event path
- `crates/assay-core/src/mcp/policy/mod.rs`
  - add approval artifact contract type
  - add approval freshness contract type
  - extend `PolicyMatchMetadata` with additive approval artifact/freshness fields

- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - thread additive approval artifact/freshness fields from policy metadata into event context

- `crates/assay-core/src/mcp/decision.rs`
  - extend Decision Event v2 data with additive approval evidence fields
  - map policy context approval artifact/freshness into event payload

- `crates/assay-core/src/mcp/proxy.rs`
  - keep proxy event emission aligned with additive approval fields

### Tests
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - verify additive compatibility (approval fields remain optional when absent)

- `crates/assay-core/src/mcp/decision.rs` (unit tests)
  - verify policy-context mapping for approval artifact fields

## Behavior parity notes
- Existing allow/deny outcomes are unchanged.
- Existing obligations execution (`log`, `alert`, `legacy_warning -> log`) is unchanged.
- No runtime enforcement of missing/expired approval is introduced.

## Out-of-scope guardrails
- no `approval_required` enforcement
- no approval UI/case management
- no external approval services
- no control-plane expansion
- no auth transport redesign
