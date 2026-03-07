# Model Step1 Review Pack (Freeze)

## Intent

Freeze Wave13 scope for `model.rs` split before any code movement.

## Scope

- `docs/contributing/SPLIT-PLAN-wave13-model.md`
- `docs/contributing/SPLIT-CHECKLIST-model-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step1.md`
- `scripts/ci/review-model-step1.sh`

## Non-goals

- no edits in `crates/assay-core/src/model.rs`
- no edits in `crates/assay-core/src/model/**`
- no behavior changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-model-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact
cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact
cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact
```

## Reviewer 60s scan

1. Confirm only Step1 docs/script changed.
2. Confirm no `model.rs` or `model/**` tracked/untracked changes.
3. Confirm helper no-IO boundary is explicit for Step2.
4. Run reviewer script and expect PASS.
