# SPLIT-MOVE-MAP — Wave16 Step2 — `mcp/tool_call_handler`

## Goal

Mechanically split `crates/assay-core/src/mcp/tool_call_handler.rs` into focused modules with zero behavior change and stable public surface.

## New layout

- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/types.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`

Legacy file removed:
- `crates/assay-core/src/mcp/tool_call_handler.rs`

## Mapping table

- Public API surface (`ToolCallHandler`, `ToolCallHandlerConfig`, `HandleResult`) -> `types.rs` and re-export in `mod.rs`.
- Public handler entrypoints (`new`, `with_lifecycle_emitter`, `handle_tool_call`) -> thin wrappers in `mod.rs`.
- Policy evaluation and mandate flow (`handle_tool_call` body and helper methods) -> `evaluate.rs`.
- Decision event construction (`DecisionEvent::new(...).allow/deny/error`) -> `emit.rs`.
- Inline tests from legacy file -> `tests.rs`.

## Frozen behavior boundaries

- `DecisionEvent` required fields and reason-code mapping unchanged.
- Commit tool mandate requirement behavior unchanged.
- Policy deny/allow routing unchanged.
- Lifecycle emission behavior (`mandate.used`) unchanged.

## Test relocation map

Moved test names unchanged:
- `test_handler_emits_decision_on_policy_deny`
- `test_handler_emits_decision_on_policy_allow`
- `test_commit_tool_without_mandate_denied`
- `test_is_commit_tool_matching`
- `test_operation_class_for_tool`
- `test_lifecycle_emitter_not_called_when_none`
