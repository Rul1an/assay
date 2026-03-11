# SPLIT-MOVE-MAP - Wave26 Step2 - Obligations Alert Execution

## Goal
Extend bounded obligations runtime handling with one additional low-risk type:
1. execute `alert` obligations
2. preserve `log` behavior
3. preserve `legacy_warning` -> `log` compatibility
4. keep `obligation_outcomes` additive

## File-level map

### Core runtime
- `crates/assay-core/src/mcp/obligations.rs`
  - extend bounded execution to include `alert`
  - keep `legacy_warning` compatibility mapping to `log`
  - keep unknown types as non-blocking `skipped`

- `crates/assay-core/src/mcp/policy/mod.rs`
  - map deny-with-alert policy decisions to typed `alert` obligations
  - preserve typed decision compatibility contract

### Tests
- `crates/assay-core/src/mcp/obligations.rs` (unit tests)
  - verify `alert` outcome behavior

- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - verify deny-with-alert path emits `alert` obligation outcomes

## Behavior parity notes
- Tool-call allow/deny end-state behavior remains unchanged.
- Alert execution remains non-blocking.
- No external incident/case-management dependency is added.

## Out-of-scope guardrails
- no `approval_required` enforcement
- no `restrict_scope` enforcement
- no `redact_args` enforcement
- no event schema redesign
