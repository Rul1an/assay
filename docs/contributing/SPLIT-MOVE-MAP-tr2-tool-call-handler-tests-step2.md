# SPLIT-MOVE-MAP — T-R2 Step2 — `mcp/tool_call_handler/tests.rs`

## Goal

Mechanically decompose `crates/assay-core/src/mcp/tool_call_handler/tests.rs` into a unit-test
module tree without changing handler behavior, private-access coverage shape, or test-family
meaning.

## New layout

- `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs`

## Mapping applied

- `tests/mod.rs`
  - module wiring only
- `fixtures.rs`
  - `CountingEmitter`
  - `make_tool_call_request`
  - `approval_required_policy`
  - `restrict_scope_policy_with_contract`
  - `restrict_scope_policy`
  - `redact_args_policy_with_contract`
  - `redact_args_policy`
  - `approval_artifact`
  - `outcome_for`
  - `assert_fail_closed_defaults`
  - `CountingLifecycleEmitter`
- `emission.rs`
  - `test_handler_emits_decision_on_policy_deny`
  - `test_handler_emits_decision_on_policy_allow`
  - `test_allow_with_warning_emits_log_obligation_outcome`
  - `test_tool_drift_deny_emits_alert_obligation_outcome`
  - `test_alert_obligation_outcome_emitted`
- `delegation.rs`
  - `delegated_context_emits_typed_fields_for_supported_flow`
  - `direct_authorization_flow_omits_delegation_fields`
  - `unstructured_delegation_hints_do_not_emit_typed_fields`
- `approval.rs`
  - all `approval_required_*`
- `scope.rs`
  - all `restrict_scope_*`
- `redaction.rs`
  - all `redact_args_*`
- `classification.rs`
  - `test_commit_tool_without_mandate_denied`
  - `test_is_commit_tool_matching`
  - `test_operation_class_for_tool`
- `lifecycle.rs`
  - `test_lifecycle_emitter_not_called_when_none`

## Frozen behavior boundaries

- identical unit-test placement under `src/mcp/tool_call_handler`
- identical private-access pattern through parent-module access
- identical handler emission, delegation, approval, scope, redaction, tool-drift, classification, and lifecycle semantics
- no integration-target conversion
- no production visibility widening

## Step2 anchor selectors

- `mcp::tool_call_handler::tests::emission::test_handler_emits_decision_on_policy_allow`
- `mcp::tool_call_handler::tests::delegation::delegated_context_emits_typed_fields_for_supported_flow`
- `mcp::tool_call_handler::tests::approval::approval_required_missing_denies`
- `mcp::tool_call_handler::tests::scope::restrict_scope_target_missing_denies`
- `mcp::tool_call_handler::tests::redaction::redact_args_target_missing_denies`
- `mcp::tool_call_handler::tests::emission::test_tool_drift_deny_emits_alert_obligation_outcome`
- `mcp::tool_call_handler::tests::classification::test_operation_class_for_tool`
- `mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none`
