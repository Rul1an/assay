# SPLIT MOVE MAP - Wave37 Decision Evidence Convergence Step2

## Intent
Bounded implementation for additive decision/evidence convergence across existing MCP runtime paths.

## Touched runtime paths
- `crates/assay-core/src/mcp/decision.rs`
  - adds additive convergence event fields:
    - `decision_outcome_kind`
    - `decision_origin`
    - `outcome_compat_state`
  - wires deterministic convergence refresh into existing normalization path
- `crates/assay-core/src/mcp/decision/outcome_convergence.rs`
  - introduces canonical convergence enums and deterministic classification logic
  - keeps policy/fail-closed/enforcement/obligation mapping explicit

## Touched tests
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - extends required-fields assertions with convergence fields
- `crates/assay-core/tests/fulfillment_normalization.rs`
  - validates deterministic convergence mapping for policy/fail-closed/enforcement paths

## Out of scope guarantees
- no new obligation types
- no new enforcement capabilities
- no policy backend/control-plane additions
- no auth transport changes
