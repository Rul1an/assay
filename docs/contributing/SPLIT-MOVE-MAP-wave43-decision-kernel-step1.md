# SPLIT MOVE MAP - Wave43 Decision Kernel Step1

## Intent
Preview the bounded Step2 move set for the `mcp/decision.rs` split without changing runtime code in
Step1.

## Allowed Step2 implementation scope
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/decision_next/mod.rs`
- `crates/assay-core/src/mcp/decision_next/event_types.rs`
- `crates/assay-core/src/mcp/decision_next/builder.rs`
- `crates/assay-core/src/mcp/decision_next/emitters.rs`
- `crates/assay-core/src/mcp/decision_next/guard.rs`
- `crates/assay-core/src/mcp/decision_next/normalization.rs`
- `crates/assay-core/src/mcp/decision_next/tests.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- `crates/assay-core/tests/fulfillment_normalization.rs`

## Move rationale
- `decision.rs`
  - becomes facade/orchestration only
- `event_types.rs`
  - owns core decision/event/data types
- `builder.rs`
  - owns allow/deny/error event construction helpers
- `emitters.rs`
  - owns file/null emitter implementations and trait placement
- `guard.rs`
  - owns single-emission invariant lifecycle
- `normalization.rs`
  - owns fulfillment normalization and projection refresh helpers
- tests
  - verify public behavior did not drift while logic moved behind the facade

## Explicitly out of scope
- policy engine changes
- tool-call handler changes
- new event fields
- reason-code changes
- CLI changes
- MCP server changes
- workflow changes
