# SPLIT MOVE MAP - Wave43 Decision Kernel Step2

## Intent
Record the exact symbol/block movement for the mechanical split of
`crates/assay-core/src/mcp/decision.rs`.

## File ownership after Step2
- `crates/assay-core/src/mcp/decision.rs`
  - stable facade
  - existing contract/replay modules
  - re-exports
  - inline unit tests
- `crates/assay-core/src/mcp/decision_next/event_types.rs`
  - `reason_codes`
  - `Decision`
  - `ObligationOutcomeStatus`
  - `ObligationOutcome`
  - `FulfillmentDecisionPath`
  - `PolicyDecisionEventContext`
  - `DecisionEvent`
  - `DecisionData`
- `crates/assay-core/src/mcp/decision_next/normalization.rs`
  - `normalize_obligation_outcome`
  - `normalize_obligation_outcomes`
  - `classify_fulfillment_decision_path`
  - `refresh_fulfillment_normalization`
  - `refresh_contract_projections`
- `crates/assay-core/src/mcp/decision_next/builder.rs`
  - `impl DecisionEvent`
  - `new`
  - `allow`
  - `deny`
  - `error`
  - `with_request_id`
  - `with_mandate`
  - `with_mandate_matches`
  - `with_latencies`
  - `with_tool_match`
  - `with_policy_context`
- `crates/assay-core/src/mcp/decision_next/emitters.rs`
  - `DecisionEmitter`
  - `FileDecisionEmitter`
  - `NullDecisionEmitter`
- `crates/assay-core/src/mcp/decision_next/guard.rs`
  - `DecisionEmitterGuard`
  - `DecisionEmitterGuard::new`
  - `set_request_id`
  - `set_mandate_info`
  - `set_mandate_matches`
  - `set_latencies`
  - `set_tool_match`
  - `set_policy_context`
  - `emit_allow`
  - `emit_deny`
  - `emit_error`
  - `emit_event`
  - `Drop for DecisionEmitterGuard`

## Explicit non-moves
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - unchanged
- `crates/assay-core/tests/fulfillment_normalization.rs`
  - unchanged
- `crates/assay-core/src/mcp/tool_call_handler/**`
  - unchanged
- `crates/assay-core/src/mcp/policy/**`
  - unchanged
- replay/consumer/context/deny modules under `crates/assay-core/src/mcp/decision/`
  - semantically unchanged
