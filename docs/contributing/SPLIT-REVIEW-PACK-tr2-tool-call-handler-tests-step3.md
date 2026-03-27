# T-R2 Tool Call Handler Tests Step3 Review Pack

## Intent

Close the T-R2 unit-test-tree split after `#984`, while keeping `tests/mod.rs` as the stable
white-box root and preserving the shipped scenario-family module tree.

## Scope

- `docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md`
- `docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step3.md`
- `scripts/ci/review-tr2-tool-call-handler-tests-step3.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no edits under `crates/assay-core/src/mcp/decision.rs`
- no production behavior changes
- no fixture reshuffle
- no new module cuts
- no unit-test to integration-test conversion

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step3.sh
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
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::classification::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none' -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step3 docs and reviewer script.
2. Confirm the plan now records Step1/Step2 as shipped on `main`.
3. Confirm the move-map reflects the Step2 module tree as the final T-R2 shape.
4. Confirm the reviewer script freezes the `tool_call_handler` test tree and reruns the module-qualified selectors.
5. Confirm there is no production or integration-test scope leakage in the closure wave.
