# Wave13 Plan - `assay-core/src/model.rs` Split

## Intent

Split `crates/assay-core/src/model.rs` into internal modules while preserving
behavior and public API contracts.

## Scope

- Step1 freeze: docs + gate script only
- Step2 mechanical: move-only split under `crates/assay-core/src/model/**`
- Step3 closure: docs + gate script only
- Step4 promote: single final promote PR (`main <- step3`)

## Public API and contract freeze

- no behavior change
- no serde shape drift for existing public types
- no rename/removal of exported model symbols

## Mechanical target layout (Step2)

- `crates/assay-core/src/model/mod.rs` (facade + public surface + re-exports)
- `crates/assay-core/src/model/types.rs` (core structs/enums)
- `crates/assay-core/src/model/serde.rs` (serde glue only)
- `crates/assay-core/src/model/validation.rs` (pure validation)
- `crates/assay-core/src/model/tests/mod.rs` (moved unit tests)

## Boundary guard (Wave12 follow-up)

- helper modules must remain pure and deterministic
- no file IO / env reads in helper modules unless explicitly planned
- if IO is required, it must live in an explicitly named module (for example `io.rs`)
  and be called out in checklist/review-pack before Step2 lands

## Step1 targeted tests (locked)

- `cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact`
- `cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact`
- `cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact`

## Promote discipline

1. PR1: Step1 `main <- step1`
2. PR2: Step2 `step1 <- step2`
3. PR3: Step3 `step2 <- step3`
4. Before final promote: merge `origin/main` into step3 branch and rerun Step3 gate
5. Final promote PR: `main <- step3`

Only enable auto-merge when `mergeStateStatus=CLEAN`.
For flaky infra failures: rerun failed checks only.
