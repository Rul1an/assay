# Wave44 Evaluate Kernel Step2 Review Pack (Mechanical Split)

## Intent

Perform the Wave44 mechanical split of `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` into focused helper modules while preserving handler behavior and emitted contracts.

## Scope

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

## Non-goals

- no workflow changes
- no changes under `crates/assay-core/tests/**`
- no deny-path redesign
- no fulfillment / replay contract changes
- no public API expansion

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave44-evaluate-kernel-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_expired_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_unsupported_match_mode_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_unsupported_scope_type_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact
```

## Reviewer 60s scan

1. Confirm diff is limited to the Step2 allowlist.
2. Confirm `mod.rs` only adds `mod evaluate_next;` and keeps the facade surface stable.
3. Confirm `evaluate.rs` still owns `handle_tool_call(...)` but no longer owns the extracted helper enums/functions.
4. Confirm `DecisionEvent::new(...)` does not appear in `evaluate.rs` or `evaluate_next/**`.
5. Confirm no tests under `crates/assay-core/tests/**` changed.
