# SPLIT MOVE MAP — Wave30 Restrict Scope Enforcement Step2

## Intent
Bounded runtime enforcement for `restrict_scope` using already-landed Wave29 contract/evidence fields.

## Code touch map
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - add `validate_restrict_scope` runtime check
  - deny with `P_RESTRICT_SCOPE` on frozen failure reasons
  - update `obligation_outcomes` for `restrict_scope` (`Applied`/`Error`)
- `crates/assay-core/src/mcp/decision.rs`
  - add reason code constant `P_RESTRICT_SCOPE`

## Test touch map
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - enforce mismatch => deny
  - enforce missing target => deny
  - enforce unsupported match mode => deny
  - enforce unsupported scope type => deny
  - keep additive-field allow path for matched scope
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - mirror the same deny/allow invariants at decision-event level

## Out-of-scope confirmations
- no rewrite/filter/redact execution
- no broad/global scope semantics
- no approval/control-plane/auth transport expansion
