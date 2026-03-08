# Mandate Types Step2 Review Pack (Mechanical)

## Intent

Mechanically split `crates/assay-evidence/src/mandate/types.rs` into bounded modules while keeping behavior and public paths stable.

## Scope

- `crates/assay-evidence/src/mandate/types.rs` (delete)
- `crates/assay-evidence/src/mandate/types/mod.rs`
- `crates/assay-evidence/src/mandate/types/core.rs`
- `crates/assay-evidence/src/mandate/types/serde.rs`
- `crates/assay-evidence/src/mandate/types/schema.rs`
- `crates/assay-evidence/src/mandate/types/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mandate-types-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step2.md`
- `scripts/ci/review-mandate-types-step2.sh`

## Non-goals

- no workflow changes
- no semantic cleanup
- no new public API surface

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-mandate-types-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_kind_serialization -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_builder -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_operation_class_serialization -- --exact
```

## Reviewer 60s scan

1. Confirm diff is only Step2 allowlist files.
2. Confirm `types/mod.rs` is thin (wiring + re-exports only).
3. Confirm tests moved to `types/tests.rs` and names preserved.
4. Run reviewer script and expect PASS.
