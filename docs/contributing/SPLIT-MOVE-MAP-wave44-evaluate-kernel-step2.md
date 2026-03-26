# SPLIT-MOVE-MAP — Wave44 Step2 — `mcp/tool_call_handler/evaluate.rs`

## Goal

Mechanically split `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` into focused helper modules with zero behavior change and stable handler entrypoint.

## New layout

- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`

## Mapping table

- Top-level evaluation flow and `handle_tool_call(...)` stay in `evaluate.rs`.
- Approval enforcement types/helpers move to `evaluate_next/approval.rs`:
  - `ApprovalFailure`
  - `validate_approval_required`
  - `mark_approval_failure`
  - `mark_approval_outcome`
  - `parse_approval_artifact`
  - `classify_approval_freshness`
- Restrict-scope enforcement types/helpers move to `evaluate_next/scope.rs`:
  - `RestrictScopeFailure`
  - `validate_restrict_scope`
  - `mark_restrict_scope_failure`
  - `mark_restrict_scope_outcome`
- Redact-args enforcement types/helpers move to `evaluate_next/redaction.rs`:
  - `RedactArgsFailure`
  - `validate_redact_args`
  - `apply_redact_args_runtime`
  - `apply_drop_redaction`
  - `redaction_target_value_mut`
  - `apply_value_redaction`
  - `partial_mask`
  - `mark_redact_args_failure`
  - `mark_redact_args_outcome`
- Fail-closed helpers move to `evaluate_next/fail_closed.rs`:
  - `seed_fail_closed_context`
  - `runtime_dependency_error_code`
  - `mark_fail_closed`
- Handler helper methods and resource extraction move to `evaluate_next/classification.rs`:
  - `requested_resource`
  - `ToolCallHandler::extract_tool_call_id`
  - `ToolCallHandler::is_commit_tool`
  - `ToolCallHandler::is_write_tool`
  - `ToolCallHandler::operation_class_for_tool`
  - `ToolCallHandler::map_policy_code_to_reason`
  - `ToolCallHandler::map_authz_error`

## Frozen behavior boundaries

- `HandleResult::{Allow,Deny,Error}` routing stays unchanged.
- Approval / restrict-scope / redact-args deny reasons and reason-codes stay unchanged.
- Obligation outcome normalization stays at handler stage `v1`.
- Fail-closed runtime dependency behavior stays unchanged.
- `tool_call_id` extraction fallback order stays unchanged.
- Public `ToolCallHandler` surface in `mod.rs` stays unchanged.
- `crates/assay-core/tests/**` remain untouched in Step2.

## Post-split shape

- `evaluate.rs`: `1016 -> 266` LOC
- helper logic is split into `evaluate_next/*` behind the same handler entrypoint
- no new public API surface added
