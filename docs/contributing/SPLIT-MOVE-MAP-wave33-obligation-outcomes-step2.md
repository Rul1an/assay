# SPLIT MOVE MAP — Wave33 Obligation Outcomes Step2

## Intent
Implement bounded, additive normalization for `obligation_outcomes` while keeping runtime policy behavior unchanged.

## Code touch map
- `crates/assay-core/src/mcp/decision.rs`
  - extend `ObligationOutcome` with additive normalization fields:
    - `reason_code`
    - `enforcement_stage`
    - `normalization_version`
- `crates/assay-core/src/mcp/obligations.rs`
  - emit normalized reason codes for executor-side outcomes
  - stamp executor stage/version markers
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - stamp handler stage/version markers
  - normalize reason-code values for approval/restrict/redact handler paths
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - assert additive normalization fields on representative allow/deny paths

## Out-of-scope confirmations
- no change to allow/deny policy decisions
- no new obligation execution type
- no approval/scope/redact semantic expansion
- no control-plane or transport-auth changes
