# SPLIT-MOVE-MAP — Wave44 Step1 — `mcp/tool_call_handler/evaluate.rs`

## Goal

Freeze the split boundaries for `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` before any mechanical module moves.

## Planned Step2 layout

- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`

## Mapping preview

- `handle_tool_call(...)` entrypoint and top-level evaluation flow stay in `evaluate.rs` as the stable facade.
- `ApprovalFailure`, `validate_approval_required`, `mark_approval_*`, `parse_approval_artifact`, and `classify_approval_freshness` move to `evaluate_next/approval.rs`.
- `RestrictScopeFailure`, `validate_restrict_scope`, and `mark_restrict_scope_*` move to `evaluate_next/scope.rs`.
- `RedactArgsFailure`, `validate_redact_args`, `apply_redact_args_runtime`, `apply_drop_redaction`, `redaction_target_value_mut`, `apply_value_redaction`, `partial_mask`, and `mark_redact_args_*` move to `evaluate_next/redaction.rs`.
- `seed_fail_closed_context`, `runtime_dependency_error_code`, and `mark_fail_closed` move to `evaluate_next/fail_closed.rs`.
- `requested_resource` plus `ToolCallHandler::{extract_tool_call_id,is_commit_tool,is_write_tool,operation_class_for_tool,map_policy_code_to_reason,map_authz_error}` move to `evaluate_next/classification.rs`.

## Frozen behavior boundaries

- `HandleResult::{Allow,Deny,Error}` routing stays unchanged.
- Decision-event additive fields for approval / restrict-scope / redact-args / fail-closed stay byte-for-byte equivalent.
- Reason-code strings and error-code mapping stay unchanged.
- Obligation outcome normalization stays at handler stage `v1`.
- `tool_call_id` extraction fallback order stays unchanged.
- Mandate authorization and lifecycle emission semantics stay unchanged.

## Test anchors to keep fixed in Step1

- `mcp::tool_call_handler::tests::approval_required_missing_denies`
- `mcp::tool_call_handler::tests::restrict_scope_target_missing_denies`
- `mcp::tool_call_handler::tests::redact_args_target_missing_denies`
- `tool_taxonomy_policy_match_handler_decision_event_records_classes`
- `fulfillment_normalizes_outcomes_and_sets_policy_deny_path`
- `classify_replay_diff_unchanged`
