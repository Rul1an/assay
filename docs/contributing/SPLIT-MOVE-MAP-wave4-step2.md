# Wave4 Step2 move map (lockfile/cache mechanical split)

Scope:
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-registry/src/cache.rs`
- `crates/assay-registry/src/lockfile_next/*`
- `crates/assay-registry/src/cache_next/*`

No public symbol path changes intended (`crate::lockfile::*`, `crate::cache::*` remain stable).

## Lockfile moves

- `Lockfile::load` -> `lockfile_next/io.rs::load_impl`
- `Lockfile::save` -> `lockfile_next/io.rs::save_impl`
- `Lockfile::parse` -> `lockfile_next/parse.rs::parse_lockfile_impl`
- `Lockfile::to_yaml` -> `lockfile_next/format.rs::to_yaml_impl`
- `Lockfile::add_pack` -> `lockfile_next/format.rs::add_pack_impl`
- `generate_lockfile` -> `lockfile_next/mod.rs::generate_lockfile_impl`
- `verify_lockfile` -> `lockfile_next/digest.rs::verify_lockfile_impl`
- `check_lockfile` -> `lockfile_next/digest.rs::check_lockfile_impl`
- `update_lockfile` -> `lockfile_next/digest.rs::update_lockfile_impl`

## Cache moves

- `PackCache::pack_dir` -> `cache_next/keys.rs::pack_dir_impl`
- `PackCache::put` -> `cache_next/put.rs::put_impl`
- `default_cache_dir` -> `cache_next/io.rs::default_cache_dir_impl`
- `parse_cache_control_expiry` -> `cache_next/policy.rs::parse_cache_control_expiry_impl`
- `parse_signature` -> `cache_next/integrity.rs::parse_signature_impl`
- `write_atomic` -> `cache_next/io.rs::write_atomic_impl`

## Drift-sensitive paths

- Lockfile stable ordering remains in one path: `lockfile_next/format.rs::add_pack_impl`.
- Atomic write/rename remains in one path: `cache_next/io.rs::write_atomic_impl`.
