# SPLIT MOVE MAP - Wave43 Decision Kernel Step3

## Intent
Close Wave43 after the shipped Step2 split and bound any follow-up work to
micro-cleanup only.

## Shipped ownership baseline
- `crates/assay-core/src/mcp/decision.rs`
  - stable public facade
  - inline unit tests
- `crates/assay-core/src/mcp/decision_next/event_types.rs`
  - public decision/event/data types and `reason_codes`
- `crates/assay-core/src/mcp/decision_next/normalization.rs`
  - fulfillment and contract projection refresh helpers
- `crates/assay-core/src/mcp/decision_next/builder.rs`
  - `DecisionEvent` builder methods
- `crates/assay-core/src/mcp/decision_next/emitters.rs`
  - emitter trait and file/null emitters
- `crates/assay-core/src/mcp/decision_next/guard.rs`
  - guard lifecycle and single-emission safety net

## Allowed future Step3 cleanup categories
- import cleanup
- internal visibility tightening
- doc comment cleanup
- tiny helper placement cleanups that do not change public behavior

## Explicitly out of scope
- payload-shape changes
- reason-code changes
- replay / contract projection changes
- new module cuts
- external test rewrites
- handler / policy / CLI / server changes
