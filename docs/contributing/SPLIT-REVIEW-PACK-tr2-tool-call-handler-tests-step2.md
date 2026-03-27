# T-R2 Tool Call Handler Tests Step2 Review Pack

## Intent

Mechanically split `crates/assay-core/src/mcp/tool_call_handler/tests.rs` into a directory-backed
unit-test tree, while preserving private access, handler behavior, and the existing white-box
meaning of the suite.

## Scope

- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs`
- `docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md`
- `docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step2.md`
- `scripts/ci/review-tr2-tool-call-handler-tests-step2.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no edits under `crates/assay-core/src/mcp/decision.rs`
- no production behavior changes
- no integration-test conversion
- no semantic reclassification of handler taxonomy

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step2.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::emission::test_handler_emits_decision_on_policy_allow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegation::delegated_context_emits_typed_fields_for_supported_flow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::scope::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redaction::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::emission::test_tool_drift_deny_emits_alert_obligation_outcome' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::classification::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none' -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the `tool_call_handler` test tree, Step2 docs, and reviewer script.
2. Confirm `tests/mod.rs` is now a thin root with only module declarations.
3. Confirm `fixtures.rs` holds shared helpers only.
4. Confirm the scenario modules read as relocation-by-family, not redesign.
5. Confirm the reviewer script re-runs module-qualified selectors across all frozen families.
