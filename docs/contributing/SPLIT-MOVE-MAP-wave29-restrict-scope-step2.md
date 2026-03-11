# SPLIT-MOVE-MAP - Wave29 Step2 - Restrict Scope Contract/Evidence

## Goal
Implement a bounded Step2 for `restrict_scope` as contract/evidence shape only:
1. typed `restrict_scope` obligation shape in policy/runtime contract
2. additive Decision Event evidence fields for scope binding
3. passive scope evaluation metadata (no blocking/enforcement)
4. compatibility retention for existing obligations and approval paths

No runtime enforcement/execution of `restrict_scope` is introduced in this step.

## File-level map

### Core policy/runtime contract
- `crates/assay-core/src/mcp/policy/mod.rs`
  - add typed restrict-scope contract shape
  - extend `ToolPolicy` with bounded restrict-scope selectors
  - extend `PolicyMatchMetadata` with additive scope/evidence fields
  - add typed `PolicyObligation::restrict_scope(...)` constructor

- `crates/assay-core/src/mcp/policy/engine.rs`
  - attach `restrict_scope` obligation on allow paths when configured
  - compute passive scope match/mismatch metadata
  - record scope fields + additive evidence fields
  - keep all behavior non-blocking for this wave

### Event/evidence propagation
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - thread scope contract/evidence fields into policy decision context

- `crates/assay-core/src/mcp/decision.rs`
  - add additive Decision Event fields:
    - `scope_type`
    - `scope_value`
    - `scope_match_mode`
    - `scope_evaluation_state`
    - `scope_failure_reason`
    - `restrict_scope_present`
    - `restrict_scope_target`
    - `restrict_scope_match`
    - `restrict_scope_reason`

- `crates/assay-core/src/mcp/proxy.rs`
  - keep proxy decision emission aligned with additive scope fields

### Obligations runtime output (bounded)
- `crates/assay-core/src/mcp/obligations.rs`
  - represent `restrict_scope` as skipped/contract-only outcome
  - do not add deny/block behavior

### Tests
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - add bounded tests for:
    - mismatch does not deny
    - match sets additive fields

- `crates/assay-core/tests/decision_emit_invariant.rs`
  - verify additive scope fields are emitted without enforcement

## Behavior parity notes
- Existing allow/deny outcomes remain unchanged for existing paths.
- Existing `log`/`alert`/`approval_required` paths remain intact.
- No `restrict_scope` enforcement, argument rewrite, or redaction is introduced.

## Out-of-scope guardrails
- no runtime `restrict_scope` enforcement
- no `restrict_scope` deny reason-code path
- no `redact_args` behavior
- no policy-backend/control-plane work
