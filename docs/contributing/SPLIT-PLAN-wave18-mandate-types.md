# Wave18 Plan — `mandate/types.rs` Split

## Goal

Split `crates/assay-evidence/src/mandate/types.rs` into bounded modules with zero behavior change and stable public API/contracts.

## Step1 (freeze)

Branch: `codex/wave18-mandate-types-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave18-mandate-types.md`
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step1.md`
- `scripts/ci/review-mandate-types-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-evidence/src/mandate/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-evidence/src/mandate/**`
- hard fail untracked files in `crates/assay-evidence/src/mandate/**`
- `cargo fmt --check`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- targeted exact tests:
  - `cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_kind_serialization -- --exact`
  - `cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_builder -- --exact`
  - `cargo test -p assay-evidence --lib mandate::types::tests::test_operation_class_serialization -- --exact`

## Step2 (mechanical split preview)

Target layout (preview):
- `crates/assay-evidence/src/mandate/types/mod.rs` (facade + public API)
- `crates/assay-evidence/src/mandate/types/core.rs`
- `crates/assay-evidence/src/mandate/types/serde.rs`
- `crates/assay-evidence/src/mandate/types/schema.rs`
- `crates/assay-evidence/src/mandate/types/tests.rs` (or `tests/mod.rs`)

Step2 principles:
- 1:1 body moves
- stable public type paths and serde surface
- no schema/invariant behavior changes
- no type or variant drift

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and keeps allowlist strict.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once chain is clean.
