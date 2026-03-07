# Model Step2 Checklist (Mechanical Split)

Scope lock:
- mechanical split of `crates/assay-core/src/model.rs` only
- docs + gate updates for Step2 only
- no workflow changes

## Required outputs

- `docs/contributing/SPLIT-CHECKLIST-model-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-model-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step2.md`
- `scripts/ci/review-model-step2.sh`

## Mechanical requirements

- facade in `crates/assay-core/src/model/mod.rs`
- split modules:
  - `crates/assay-core/src/model/types.rs`
  - `crates/assay-core/src/model/serde.rs`
  - `crates/assay-core/src/model/validation.rs`
  - `crates/assay-core/src/model/tests/mod.rs`
- no behavior drift and no public API path drift
- tests moved with identical names/assertions
- no IO in `serde.rs` and `validation.rs`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact`
- `cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact`
- `cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact`
- allowlist-only diff
- workflow-ban
- facade invariants
- module boundary invariants

## Definition of done

- reviewer script passes with `BASE_REF=origin/codex/wave13-model-step1-freeze`
- diff is limited to Step2 allowlist files
