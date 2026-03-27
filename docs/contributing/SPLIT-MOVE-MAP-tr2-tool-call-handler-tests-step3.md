# SPLIT-MOVE-MAP - T-R2 Step3 - `mcp/tool_call_handler/tests.rs`

## Goal

Close T-R2 after the shipped Step2 module-tree split without changing handler behavior,
private-access coverage shape, or the white-box meaning of the suite.

## Final layout retained

- `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs`

## Closure assertions

- `tests/mod.rs` remains the stable unit-test root and stays thin
- the split remains a `src` unit-test tree, not an integration target
- the Step2 family boundaries remain intact:
  - `fixtures`
  - `emission`
  - `delegation`
  - `approval`
  - `scope`
  - `redaction`
  - `classification`
  - `lifecycle`
- future helper cleanup or selector hygiene that reaches beyond closure-only docs/gates requires a separate wave
- no private-access widening, production behavior cleanup, or test-family reinterpretation is part of Step3

## Closure anchor selectors

- `mcp::tool_call_handler::tests::emission::test_handler_emits_decision_on_policy_allow`
- `mcp::tool_call_handler::tests::delegation::delegated_context_emits_typed_fields_for_supported_flow`
- `mcp::tool_call_handler::tests::approval::approval_required_missing_denies`
- `mcp::tool_call_handler::tests::scope::restrict_scope_target_missing_denies`
- `mcp::tool_call_handler::tests::redaction::redact_args_target_missing_denies`
- `mcp::tool_call_handler::tests::classification::test_operation_class_for_tool`
- `mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none`
