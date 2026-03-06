# Mandate Store Step 2 Review Pack (Mechanical Test Extraction)

## Intent

Move mandate-store tests out of `mandate_store.rs` into `mandate_store_next/tests.rs`
without changing behavior or contracts.

## Scope

- `crates/assay-core/src/runtime/mandate_store.rs`
- `crates/assay-core/src/runtime/mandate_store_next/mod.rs`
- `crates/assay-core/src/runtime/mandate_store_next/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-mandate-store-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mandate-store-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step2.md`
- `scripts/ci/review-mandate-store-step2.sh`

## Non-goals

- No production logic changes.
- No SQL transaction boundary changes.
- No workflow changes.

## Validation command

```bash
BASE_REF=<step1-branch-or-main> bash scripts/ci/review-mandate-store-step2.sh
```

## Reviewer 60s scan

1. Confirm `mandate_store.rs` no longer contains inline `mod tests { ... }`.
2. Confirm all test bodies are now in `mandate_store_next/tests.rs`.
3. Confirm `mandate_store_next/mod.rs` no longer declares `mod tests;` (single load path).
4. Confirm transaction-boundary single-source check still points only to `mandate_store_next/txn.rs`.
5. Run reviewer script and confirm PASS.
