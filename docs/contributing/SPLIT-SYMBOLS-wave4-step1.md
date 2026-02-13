# Wave4 Step1 symbol snapshot (registry lockfile/cache)

## `crates/assay-registry/src/lockfile.rs`

Public types:
- `Lockfile`
- `LockedPack`
- `LockSource`
- `LockSignature`
- `VerifyLockResult`
- `LockMismatch`

Public API surface (impl + free fns):
- `Lockfile::new`
- `Lockfile::load`
- `Lockfile::parse`
- `Lockfile::save`
- `Lockfile::to_yaml`
- `Lockfile::add_pack`
- `Lockfile::remove_pack`
- `Lockfile::get_pack`
- `Lockfile::contains`
- `Lockfile::pack_names`
- `generate_lockfile`
- `verify_lockfile`
- `check_lockfile`
- `update_lockfile`

## `crates/assay-registry/src/cache.rs`

Public types:
- `CacheMeta`
- `PackCache`
- `CacheEntry`

Public API surface (impl):
- `PackCache::new`
- `PackCache::with_dir`
- `PackCache::cache_dir`
- `PackCache::get`
- `PackCache::put`
- `PackCache::get_metadata`
- `PackCache::get_etag`
- `PackCache::is_cached`
- `PackCache::evict`
- `PackCache::clear`
- `PackCache::list`

Intent:
- Keep these symbols and semantics stable in Step1; only freeze/gates/docs are added.
