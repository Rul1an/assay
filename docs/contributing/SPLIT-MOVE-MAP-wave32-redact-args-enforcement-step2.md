# SPLIT MOVE MAP — Wave32 Redact Args Enforcement Step2

## Intent
Bounded runtime enforcement for `redact_args` using the already-landed Wave31 shape/evidence fields.

## Code touch map
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - add `validate_redact_args` runtime check
  - deny with `P_REDACT_ARGS` on frozen failure reasons
  - update `obligation_outcomes` for `redact_args` (`Applied`/`Error`)
- `crates/assay-core/src/mcp/policy/engine.rs`
  - normalize redaction evaluation to `applied | not_applied | not_evaluated`
  - emit deterministic redaction failure reasons
- `crates/assay-core/src/mcp/policy/mod.rs`
  - add additive `redaction_failure_reason` metadata field
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - carry `redaction_failure_reason` into event policy context
- `crates/assay-core/src/mcp/decision.rs`
  - add reason code constant `P_REDACT_ARGS`
  - add additive event field `redaction_failure_reason`
- `crates/assay-core/src/mcp/proxy.rs`
  - pass through `redaction_failure_reason` in emitted decision data

## Test touch map
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - update allow-path redaction expectations to runtime-enforced shape
  - add deny-path tests for missing target, unsupported mode/scope, apply-failed
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - mirror deny/allow invariants at decision-event level

## Out-of-scope confirmations
- no broad/global scrub policy semantics
- no PII/classifier engine
- no external DLP orchestration
- no approval/restrict_scope behavior expansion
