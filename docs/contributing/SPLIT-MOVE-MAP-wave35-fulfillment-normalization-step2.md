# SPLIT MOVE MAP - Wave35 Fulfillment Normalization Step2

## Intent
Bounded implementation for additive obligation fulfillment normalization across existing MCP runtime paths.

## Touched runtime paths
- `crates/assay-core/src/mcp/decision.rs`
  - adds deterministic normalization defaults (`reason_code`, `enforcement_stage`, `normalization_version`)
  - adds additive fulfillment-path classification (`policy_allow`, `policy_deny`, `fail_closed_deny`, `decision_error`)
  - adds additive outcome-presence flags (`obligation_applied_present`, `obligation_skipped_present`, `obligation_error_present`)
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - updates assertions to validate normalized outcome reason codes and fulfillment-path mapping

## Touched tests
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - extends invariants with additive normalization fields and compat expectations
- `crates/assay-core/tests/fulfillment_normalization.rs`
  - validates deterministic normalization defaults and policy-vs-fail-closed deny separation

## Out of scope guarantees
- no new obligation types
- no new policy backend semantics
- no control-plane additions
- no auth transport changes
- no additional execution workflows beyond existing obligations
