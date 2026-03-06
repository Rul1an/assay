# Mandate store split Step 2 checklist (mechanical test extraction)

Scope lock:
- mechanical split only
- no behavior changes
- no SQL/policy rewrites
- no workflow changes

## Goal

Extract inline `#[cfg(test)] mod tests` from `crates/assay-core/src/runtime/mandate_store.rs`
into `crates/assay-core/src/runtime/mandate_store_next/tests.rs` while keeping test names,
assertions, fixtures, and contracts identical.

## Target files

- `crates/assay-core/src/runtime/mandate_store.rs`
- `crates/assay-core/src/runtime/mandate_store_next/mod.rs`
- `crates/assay-core/src/runtime/mandate_store_next/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-mandate-store-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mandate-store-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step2.md`
- `scripts/ci/review-mandate-store-step2.sh`

## Hard gates

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-core`
- contract tests:
  - `cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture`
  - `cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture`
  - `cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture`
  - `cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture`
- boundary checks:
  - no inline `mod tests {` remains in `mandate_store.rs`
  - transaction boundaries (`BEGIN/COMMIT/ROLLBACK/transaction`) remain single-source in `mandate_store_next/txn.rs`
  - facade file `mandate_store.rs` contains no SQL statement bodies
- diff allowlist only
- workflow-ban (`.github/workflows/*` forbidden)

## Acceptatiecriteria

- `mandate_store.rs` is a thin production facade plus `#[cfg(test)]` path module include
- all moved tests compile and run from `mandate_store_next/tests.rs`
- no API/behavior contract drift
- reviewer script passes end-to-end
