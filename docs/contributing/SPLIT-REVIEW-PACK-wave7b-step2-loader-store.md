# Wave7B Step2 review pack: loader/store mechanical split

Intent:
- Mechanically split large helper/orchestration blocks from:
  - `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader.rs`
  - `/Users/roelschuurkes/assay/crates/assay-core/src/storage/store.rs`
- Keep public behavior and signatures stable behind existing facades.

Executed validation:
```bash
cargo fmt --check
cargo clippy -p assay-evidence -p assay-core --all-targets -- -D warnings
cargo check -p assay-evidence -p assay-core
BASE_REF=origin/main bash scripts/ci/review-wave7b-step2.sh
```

Targeted anchor checks (also in reviewer script):
```bash
cargo test -p assay-evidence test_local_pack_resolution -- --exact
cargo test -p assay-evidence test_builtin_wins_over_local -- --exact
cargo test -p assay-evidence test_local_invalid_yaml_fails -- --exact
cargo test -p assay-evidence test_resolution_order_mock -- --exact
cargo test -p assay-evidence test_path_wins_over_builtin -- --exact
cargo test -p assay-evidence test_symlink_escape_rejected -- --exact
cargo test -p assay-core --test storage_smoke test_storage_smoke_lifecycle -- --exact
cargo test -p assay-core --test store_consistency_e1 e1_runs_write_contract_insert_and_create -- --exact
cargo test -p assay-core --test store_consistency_e1 e1_latest_run_selection_is_id_based_not_timestamp_string -- --exact
cargo test -p assay-core --test store_consistency_e1 e1_stats_read_compat_keeps_legacy_started_at -- --exact
```

Facade proof snippets:
```rust
// loader.rs
pub fn load_pack(reference: &str) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_impl(reference)
}
```

```rust
// store.rs
fn migrate_v030(conn: &Connection) -> anyhow::Result<()> {
    store_internal::schema::migrate_v030_impl(conn)
}
```

LOC snapshot:
- `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader.rs`: `793 -> 467`
- `/Users/roelschuurkes/assay/crates/assay-core/src/storage/store.rs`: `774 -> 658`

Risk:
- Medium-low: mechanical internal extraction only; no public API changes; anchors and boundary gates enforce parity.
