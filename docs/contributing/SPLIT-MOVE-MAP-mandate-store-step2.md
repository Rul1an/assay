# Mandate store Step 2 move map (mechanical test extraction)

## Scope

- Source: `crates/assay-core/src/runtime/mandate_store.rs`
- Destination: `crates/assay-core/src/runtime/mandate_store_next/tests.rs`

## Moves

| Old location | New location | Notes |
| --- | --- | --- |
| Inline `#[cfg(test)] mod tests` block in `mandate_store.rs` | `mandate_store_next/tests.rs` | Pure move, no logic edits |
| `mandate_store_next/mod.rs` `mod tests;` declaration | removed | Prevent duplicate module load; tests are now compiled only through facade path include |
| `test_metadata` helper | `mandate_store_next/tests.rs` | Signature unchanged |
| `consume` helper | `mandate_store_next/tests.rs` | Signature unchanged |
| All test functions (`test_store_bootstraps_schema`, `test_compute_use_id_contract_vector`, etc.) | `mandate_store_next/tests.rs` | Names/asserts unchanged |

## Facade wiring

`mandate_store.rs` now keeps only:

```rust
#[cfg(test)]
#[path = "mandate_store_next/tests.rs"]
mod tests;
```

No public API symbol changes in this step.
