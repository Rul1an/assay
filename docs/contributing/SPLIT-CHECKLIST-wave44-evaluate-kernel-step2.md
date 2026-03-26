# Wave44 Evaluate Kernel Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/fail_closed.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs`
- `docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md`
- `docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step2.md`
- `scripts/ci/review-wave44-evaluate-kernel-step2.sh`
- no edits under `crates/assay-core/tests/**`
- no workflow edits

## Mechanical invariants

- `mod.rs` stays facade-only and adds only `mod evaluate_next;` wiring.
- `evaluate.rs` keeps `handle_tool_call(...)` and top-level routing only.
- `evaluate.rs` no longer owns:
  - `ApprovalFailure`
  - `RestrictScopeFailure`
  - `RedactArgsFailure`
  - `validate_approval_required`
  - `validate_restrict_scope`
  - `validate_redact_args`
  - `requested_resource`
  - `seed_fail_closed_context`
  - `runtime_dependency_error_code`
  - `mark_fail_closed`
  - `impl ToolCallHandler`
- `evaluate_next/approval.rs` owns approval validation and freshness parsing.
- `evaluate_next/scope.rs` owns restrict-scope validation and outcome marking.
- `evaluate_next/redaction.rs` owns redact-args runtime mutation and outcome marking.
- `evaluate_next/fail_closed.rs` owns fail-closed context seeding and runtime dependency error marking.
- `evaluate_next/classification.rs` owns `ToolCallHandler` helper methods and `requested_resource`.
- `DecisionEvent::new(...)` remains outside `evaluate.rs` and `evaluate_next/**`.

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/tests/**`
- hard fail untracked files in `crates/assay-core/tests/**`
- hard fail untracked files under `crates/assay-core/src/mcp/tool_call_handler/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `mcp::tool_call_handler::tests::approval_required_missing_denies`
  - `mcp::tool_call_handler::tests::approval_required_expired_denies`
  - `mcp::tool_call_handler::tests::restrict_scope_target_missing_denies`
  - `mcp::tool_call_handler::tests::restrict_scope_unsupported_match_mode_denies`
  - `mcp::tool_call_handler::tests::restrict_scope_unsupported_scope_type_denies`
  - `mcp::tool_call_handler::tests::redact_args_target_missing_denies`
  - `tool_taxonomy_policy_match_handler_decision_event_records_classes`
  - `fulfillment_normalizes_outcomes_and_sets_policy_deny_path`
  - `classify_replay_diff_unchanged`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-wave44-evaluate-kernel-step2.sh` passes
- split remains behavior-identical (no deny/fulfillment/replay drift)
- `evaluate.rs` is reduced to facade/routing logic and helper modules carry the extracted bodies
- no tests under `crates/assay-core/tests/**` changed
