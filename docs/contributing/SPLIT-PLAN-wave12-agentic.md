# Wave12 Plan - `assay-core/agentic` Split

## Intent

Split `crates/assay-core/src/agentic/mod.rs` into internal modules while preserving
public API and behavior.

## Scope

- Step1 freeze: docs + gate script only
- Step2 mechanical: module split under `crates/assay-core/src/agentic/**`
- Step3 closure: docs + gate script only
- Step4 promote: single final promote PR (`main <- step3`)

## Public API freeze

The following public surface must remain unchanged:

- `pub enum RiskLevel`
- `pub struct SuggestedAction`
- `pub struct SuggestedPatch`
- `pub enum JsonPatchOp`
- `pub struct AgenticCtx`
- `pub fn build_suggestions`

## Mechanical target layout (Step2)

- `crates/assay-core/src/agentic/mod.rs` (facade + public surface)
- `crates/assay-core/src/agentic/builder.rs` (`pub(crate) fn build_suggestions_impl`)
- `crates/assay-core/src/agentic/policy_helpers.rs` (pure pointer/policy helpers)
- `crates/assay-core/src/agentic/tests/mod.rs` (moved unit tests)

## Contracts and invariants

- no behavior drift in suggestions generation
- JSON Pointer helper semantics remain RFC6901-compatible (`~0`, `~1`)
- `mod.rs` remains thin and delegates to `builder::build_suggestions_impl(...)`
- original 5 unit test names remain present

## Promote discipline

1. PR1: Step1 `main <- step1`
2. PR2: Step2 `step1 <- step2`
3. PR3: Step3 `step2 <- step3`
4. Before final promote: merge `origin/main` into step3 branch and rerun Step3 gate
5. Final promote PR: `main <- step3`

Only enable auto-merge when `mergeStateStatus=CLEAN`.
For flaky infra failures: rerun failed checks only.
