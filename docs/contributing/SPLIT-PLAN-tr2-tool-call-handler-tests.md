# T-R2 Plan — `mcp/tool_call_handler/tests.rs` Unit Test Tree Decomposition

## Goal

Split `crates/assay-core/src/mcp/tool_call_handler/tests.rs` into a unit-test module tree without
changing handler behavior, private-access coverage shape, or the white-box meaning of the suite.

T-R2 Step1 shipped on `main` via `#983`.
T-R2 Step2 shipped on `main` via `#984`.

This plan intentionally follows Rust unit-test conventions:

- keep this suite in `src/`
- keep it under the existing `#[cfg(test)] mod tests;` surface
- decompose it as `tests/mod.rs` plus submodules
- preserve private access through `super::*`

Current hotspot baseline on `origin/main @ d3f03618`:

- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`: `1242` LOC
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`: already split in Wave44 and now the closest behavior companion
- `crates/assay-core/src/mcp/decision.rs`: already split in Wave43 and now the closest emitted-event companion
- `crates/assay-core/src/mcp/policy/engine.rs`: already split in Wave45 and now the closest policy companion

## Why this plan exists

`tool_call_handler/tests.rs` is large, but it is not an integration target. It is a **white-box
unit-test surface** that exists specifically to validate internal behavior with private-access
visibility.

That means the right split shape is:

- keep the tests in `src/`
- keep private access through `super::*`
- replace one large `tests.rs` with `tests/mod.rs` plus scenario-family submodules
- do not convert this suite into integration tests

## Frozen target surface

T-R2 freezes the expectation that any later split keeps these unit-test properties stable:

- the suite remains under `crates/assay-core/src/mcp/tool_call_handler`
- the suite continues to use unit-test/private-access placement
- the existing helper/setup intent remains stable:
  - `CountingEmitter`
  - `make_tool_call_request`
  - `approval_required_policy`
  - `restrict_scope_policy_with_contract`
  - `redact_args_policy_with_contract`
  - `approval_artifact`
  - `outcome_for`
  - `assert_fail_closed_defaults`
  - `CountingLifecycleEmitter`
- the current internal behavior families remain stable in meaning:
  - handler decision emission
  - delegation typed-field projection
  - approval-required deny paths
  - restrict-scope deny/additive-field paths
  - redact-args deny/additive-field paths
  - tool drift / obligation outcome behavior
  - commit-tool classification helpers
  - lifecycle emitter behavior

## Step1 (freeze)

Step1 should be docs/gates only.

Step1 constraints:

- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/decision.rs`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no workflow edits

Step1 gate should pin representative tests from each family, for example:

- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_handler_emits_decision_on_policy_allow' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegated_context_emits_typed_fields_for_supported_flow' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_tool_drift_deny_emits_alert_obligation_outcome' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_operation_class_for_tool' -- --exact`
- `cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_lifecycle_emitter_not_called_when_none' -- --exact`

## Step2 (mechanical split preview)

Step2 should replace the single `tests.rs` file with a directory-backed unit-test module tree
while keeping the parent module declaration unchanged.

Target layout:

- `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs`

Step2 principles:

- keep the suite a unit-test tree, not an integration-test target
- keep the parent `mod tests;` declaration stable
- keep `mod.rs` thin: module wiring, shared imports, and only the smallest common prelude needed for private access
- do not leave the bulk of test bodies in `mod.rs`
- move helper/setup code into `fixtures.rs`
- move test bodies by scenario family
- preserve private access through `super::*`
- module decomposition must preserve the existing private-access pattern; submodules should continue to rely on `super::*` / parent-module access rather than forcing visibility widening in production code

Step2 family ownership:

- `fixtures.rs`: shared emitters, request builders, policy builders, approval artifacts, and outcome helpers only; no scenario-specific assertions or family-owned behavior unless genuinely reused across families
- `emission.rs`: handler decision emission and obligation outcome tests only
- `delegation.rs`: delegated/direct/unstructured delegation tests
- `approval.rs`: `approval_required_*`
- `scope.rs`: `restrict_scope_*`
- `redaction.rs`: `redact_args_*`
- `classification.rs`: existing commit-tool and operation-class helper tests only; it must not become a place to reinterpret or reorganize handler taxonomy semantics during the split
- `lifecycle.rs`: lifecycle-emitter-specific behavior only

## Step3 (closure)

Step3 should be docs/gates only.

Step3 constraints:

- keep `tests/mod.rs` as the stable unit-test root
- no promotion into `crates/assay-core/tests/**`
- no production behavior cleanup inside handler/decision/policy code
- no new module cuts
- no drift in private-access coverage shape
- no selector churn beyond closure-gate alignment with the shipped module tree

## Reviewer notes

Primary failure modes:

- converting a white-box unit-test suite into an integration-test suite
- accidentally changing production behavior while “just moving tests”
- duplicating fixtures across many files instead of centralizing them
- letting private-access assumptions drift because imports or module roots changed
- mixing new handler semantics into a test decomposition wave

## Non-goals

- No production edits under `crates/assay-core/src/mcp/tool_call_handler/*.rs`.
- No edits under `crates/assay-core/tests/**`.
- No conversion to integration tests.
- No new handler/evaluate/policy semantics.
- No assertion or helper cleanup beyond strictly mechanical relocation.
