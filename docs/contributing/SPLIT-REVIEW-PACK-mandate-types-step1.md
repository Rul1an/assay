# Mandate Types Step1 Review Pack (Freeze)

## Intent

Freeze Wave18 scope for `crates/assay-evidence/src/mandate/types.rs` before any mechanical moves.

## Scope

- `docs/contributing/SPLIT-PLAN-wave18-mandate-types.md`
- `docs/contributing/SPLIT-CHECKLIST-mandate-types-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step1.md`
- `scripts/ci/review-mandate-types-step1.sh`

## Non-goals

- no changes under `crates/assay-evidence/src/mandate/**`
- no workflow changes
- no behavior or API changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-mandate-types-step1.sh
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

1. Confirm diff is only the 4 Step1 files.
2. Confirm workflow-ban and mandate subtree bans exist in the script.
3. Confirm targeted tests are pinned with `--exact`.
4. Run reviewer script and expect PASS.
