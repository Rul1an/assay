# Mandate store split Step 3 checklist (closure)

Scope lock:
- closure/docs/gates only
- no production Rust edits
- no workflow edits

## Goal

Finalize the mandate-store split with strict closure gates:
- keep `mandate_store.rs` as thin facade
- keep tests in `mandate_store_next/tests.rs`
- freeze transaction boundary and SQL containment
- enforce docs-only diff scope for Step3

## Final layout invariants

- `crates/assay-core/src/runtime/mandate_store.rs` keeps only public API/types + path-based test include
- `crates/assay-core/src/runtime/mandate_store_next/tests.rs` contains moved test bodies
- `crates/assay-core/src/runtime/mandate_store_next/mod.rs` does not declare `mod tests;`
- transaction boundary (`BEGIN/COMMIT/ROLLBACK/transaction`) remains only in `mandate_store_next/txn.rs`

## LOC targets (regression guard)

- `mandate_store.rs` line count stays `<= 250`
- `mandate_store_next/tests.rs` line count stays `>= 500`

## Required checks

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-core`
- contract tests:
  - `cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture`
  - `cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture`
  - `cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture`
  - `cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture`
- `BASE_REF=<step2-base> bash scripts/ci/review-mandate-store-step3.sh`

## Diff allowlist (Step3)

- `docs/contributing/SPLIT-CHECKLIST-mandate-store-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mandate-store-step3.md`
- `scripts/ci/review-mandate-store-step3.sh`

## Acceptatiecriteria

- closure script passes
- Step3 diff is docs/script only
- no contract regressions
