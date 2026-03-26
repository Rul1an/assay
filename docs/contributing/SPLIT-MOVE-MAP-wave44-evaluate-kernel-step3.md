# SPLIT-MOVE-MAP — Wave44 Step3 — `mcp/tool_call_handler/evaluate.rs`

## Goal

Close the shipped Wave44 Step2 split without reopening `tool_call_handler/**` code movement.

## Shipped layout (frozen for Step3)

- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`

## Ownership freeze

- `evaluate.rs` remains the stable facade and `handle_tool_call(...)` entrypoint.
- `evaluate_next/approval.rs` remains the approval enforcement boundary.
- `evaluate_next/scope.rs` remains the restrict-scope enforcement boundary.
- `evaluate_next/redaction.rs` remains the redact-args enforcement boundary.
- `evaluate_next/fail_closed.rs` remains the fail-closed helper boundary.
- `evaluate_next/classification.rs` remains the request/resource/classification helper boundary.

## Allowed Step3 follow-up

- internal visibility tightening only if it requires no code edits in this wave
- docs/review-pack clarification only
- reviewer gate tightening only

## Forbidden Step3 drift

- edits to `crates/assay-core/src/mcp/tool_call_handler/**`
- edits to `crates/assay-core/tests/**`
- payload-shape changes
- deny-path redesign
- fulfillment normalization changes
- replay classification changes
- new module splits or ownership reshuffles
