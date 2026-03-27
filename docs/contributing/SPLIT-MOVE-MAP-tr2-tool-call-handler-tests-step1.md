# SPLIT-MOVE-MAP — T-R2 Step1 — `mcp/tool_call_handler/tests.rs`

## Goal

Freeze the split boundaries for `crates/assay-core/src/mcp/tool_call_handler/tests.rs` before any
mechanical module moves.

## Planned Step2 layout

- `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs`

## Mapping preview

- `tests/mod.rs` keeps the stable unit-test root, module wiring, shared imports, and only the
  smallest common prelude needed for private access.
- `fixtures.rs` is the planned home for shared emitters, request builders, policy builders,
  approval artifacts, outcome helpers, and lifecycle helpers only.
- `emission.rs` is the planned home for:
  - handler decision emission
  - allow/deny contract anchors
  - obligation outcome emission
  - fail-closed default assertions tied to emitted events
- `delegation.rs` is the planned home for:
  - delegated/direct/unstructured delegation projections
- `approval.rs` is the planned home for:
  - `approval_required_*`
- `scope.rs` is the planned home for:
  - `restrict_scope_*`
- `redaction.rs` is the planned home for:
  - `redact_args_*`
- `classification.rs` is the planned home for:
  - existing commit-tool and operation-class helper tests only
- `lifecycle.rs` is the planned home for:
  - lifecycle-emitter-specific behavior

## Frozen behavior boundaries

- identical unit-test placement under `src/mcp/tool_call_handler`
- identical private-access coverage through `super::*`
- identical handler decision emission meaning
- identical delegation typed-field projection expectations
- identical approval/restrict-scope/redact-args deny semantics
- identical tool-drift and obligation outcome semantics
- identical commit-tool / operation-class helper expectations
- identical lifecycle emitter behavior

## Test anchors to keep fixed in Step1

- `mcp::tool_call_handler::tests::test_handler_emits_decision_on_policy_allow`
- `mcp::tool_call_handler::tests::delegated_context_emits_typed_fields_for_supported_flow`
- `mcp::tool_call_handler::tests::approval_required_missing_denies`
- `mcp::tool_call_handler::tests::restrict_scope_target_missing_denies`
- `mcp::tool_call_handler::tests::redact_args_target_missing_denies`
- `mcp::tool_call_handler::tests::test_tool_drift_deny_emits_alert_obligation_outcome`
- `mcp::tool_call_handler::tests::test_operation_class_for_tool`
- `mcp::tool_call_handler::tests::test_lifecycle_emitter_not_called_when_none`
