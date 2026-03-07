# Wave15 Plan - `mcp/policy.rs` Split

## Intent

Split `crates/assay-core/src/mcp/policy.rs` into bounded modules while preserving behavior and public policy contract.

## Scope

- Step1 freeze: docs + reviewer gate script only
- Step2 mechanical: move-only split under `crates/assay-core/src/mcp/policy/**`
- Step3 closure: docs + reviewer gate script only
- Step4 promote: single final promote PR (`main <- step3`)

## Public contract freeze

- `McpPolicy` behavior remains unchanged
- taxonomy/class matching behavior remains unchanged
- decision emission invariants remain unchanged
- legacy normalization/migration behavior remains unchanged

## Step1 targeted checks (locked)

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
- `cargo test -p assay-core test_event_contains_required_fields -- --exact`
- `cargo test -p assay-core test_mixed_tools_config -- --exact`

## Mechanical target layout (Step2)

- `crates/assay-core/src/mcp/policy/mod.rs` (facade + public surface)
- `crates/assay-core/src/mcp/policy/engine.rs` (evaluation path)
- `crates/assay-core/src/mcp/policy/schema.rs` (schema/validation helpers)
- `crates/assay-core/src/mcp/policy/normalize.rs` (legacy shape normalization)
- `crates/assay-core/src/mcp/policy/state.rs` (state helpers)
- `crates/assay-core/src/mcp/policy/tests/mod.rs` (moved tests)

## Step2 invariants to enforce

- `mod.rs` remains thin and keeps public surface stable
- no policy-engine logic left in facade
- wrappers map 1:1 to internal implementation entrypoints
- no inline tests in facade
- no workflow changes

## Promote discipline

1. PR1: Step1 `main <- step1`
2. PR2: Step2 `step1 <- step2`
3. PR3: Step3 `step2 <- step3`
4. Before final promote: merge `origin/main` into step3 branch and rerun Step3 gate
5. Final promote PR: `main <- step3`

Only enable auto-merge when `mergeStateStatus=CLEAN`.
For flaky infra failures: rerun failed checks only.
