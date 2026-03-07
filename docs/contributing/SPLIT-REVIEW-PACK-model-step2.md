# Model Step2 Review Pack (Mechanical Split)

## Intent

Perform the Wave13 mechanical split of `model.rs` into bounded modules with
stable public surface and no behavior drift.

## Scope

- `crates/assay-core/src/model.rs` (deleted during transition)
- `crates/assay-core/src/model/mod.rs`
- `crates/assay-core/src/model/types.rs`
- `crates/assay-core/src/model/serde.rs`
- `crates/assay-core/src/model/validation.rs`
- `crates/assay-core/src/model/tests/mod.rs`
- `docs/contributing/SPLIT-CHECKLIST-model-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-model-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step2.md`
- `scripts/ci/review-model-step2.sh`

## Non-goals

- no workflow changes
- no unrelated crate edits
- no model behavior redesign

## Validation

```bash
BASE_REF=origin/codex/wave13-model-step1-freeze bash scripts/ci/review-model-step2.sh
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

1. Confirm diff stays inside Step2 allowlist.
2. Confirm facade is thin and wiring-only.
3. Confirm no IO in `model/serde.rs` and `model/validation.rs`.
4. Confirm tests relocated to `model/tests/mod.rs`.
5. Run reviewer script and expect PASS.
