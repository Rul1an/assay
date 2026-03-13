# SPLIT MOVE MAP - Wave36 Redact Args Enforcement Step2

## Intent
Bounded runtime implementation for `redact_args` execution, aligned with Wave36 hardening semantics.

## Touched runtime paths
- `crates/assay-core/src/mcp/tool_call_handler/types.rs`
  - adds `effective_arguments` to allow-result contract (additive)
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - plumbs `effective_arguments` into allow-result emission
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - introduces runtime redaction application for bounded targets/modes
  - preserves deterministic deny mapping for unsupported/failed redaction
  - keeps additive evidence + normalized obligation outcomes intact

## Touched tests
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - validates runtime redaction effect via `effective_arguments`
  - validates deterministic outcome reason-code for applied path
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - validates additive evidence + runtime redaction in integration path

## Out of scope guarantees
- no new obligation types
- no policy-engine backend changes
- no control-plane additions
- no auth transport changes
- no external DLP/PII integrations
