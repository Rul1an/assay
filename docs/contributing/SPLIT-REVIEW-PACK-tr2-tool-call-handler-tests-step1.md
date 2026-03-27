# T-R2 Tool Call Handler Tests Step1 Review Pack

## Intent

Freeze the split boundaries for `crates/assay-core/src/mcp/tool_call_handler/tests.rs` before any
mechanical unit-test decomposition, while preserving the suite as one white-box `src`-local test
tree with private access.

## Scope

- `docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md`
- `docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step1.md`
- `scripts/ci/review-tr2-tool-call-handler-tests-step1.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/decision.rs`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no production behavior changes
- no conversion into integration tests
- no visibility widening in production code

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step1.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_handler_emits_decision_on_policy_allow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegated_context_emits_typed_fields_for_supported_flow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_tool_drift_deny_emits_alert_obligation_outcome' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_lifecycle_emitter_not_called_when_none' -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the 5 Step1 files.
2. Confirm the plan keeps the suite in `src/` as a unit-test tree with private access.
3. Confirm `tests/mod.rs` is planned as a thin root rather than another giant test file.
4. Confirm `fixtures.rs` is constrained to genuinely shared helpers only.
5. Confirm the reviewer script re-runs emission, delegation, approval, scope, redaction, tool-drift, classification, and lifecycle anchors.
