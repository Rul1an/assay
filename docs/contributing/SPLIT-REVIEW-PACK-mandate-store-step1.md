# Mandate Store Step 1 Review Pack (Behavior Freeze)

## Intent

Freeze the split boundary for `mandate_store` before any mechanical modularization. This step is docs+gate only.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-mandate-store-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step1.md`
- `scripts/ci/review-mandate-store-step1.sh`

## Non-goals

- No mechanical split yet.
- No logic/performance changes.
- No workflow changes.

## Frozen Target

- `crates/assay-core/src/runtime/mandate_store.rs`

## Validation Command

```bash
BASE_REF=<previous-step-commit> bash scripts/ci/review-mandate-store-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture
```

## Reviewer 60s Scan

1. Confirm step scope is docs/script only.
2. Confirm `mandate_store.rs` has no edits in this step.
3. Confirm drift no-increase gates are code-only (exclude `#[cfg(test)]` section).
4. Run reviewer script and confirm PASS.
