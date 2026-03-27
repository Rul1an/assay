# Wave48 Step2 Move Map

Facade retained in `crates/assay-registry/src/trust.rs`:

- `TrustStore`
- `KeyMetadata`
- `TrustStore::new`
- `TrustStore::from_pinned_roots`
- `TrustStore::with_pinned_roots`
- `TrustStore::from_production_roots`
- `TrustStore::with_production_roots`
- `TrustStore::add_pinned_key`
- `TrustStore::add_from_manifest`
- `TrustStore::get_key_async`
- `TrustStore::get_key`
- `TrustStore::needs_refresh`
- `TrustStore::is_trusted`
- `TrustStore::list_keys`
- `TrustStore::get_metadata`
- `TrustStore::clear_cached_keys`
- existing inline tests
- `#[path = "trust_next/mod.rs"] mod trust_next;`

Moved to `crates/assay-registry/src/trust_next/decode.rs`:

- `decode_verifying_key`
- `decode_public_key_bytes`

Moved to `crates/assay-registry/src/trust_next/pinned.rs`:

- `parse_pinned_roots_json_impl`
- `load_production_roots_impl`
- `insert_pinned_key`

Moved to `crates/assay-registry/src/trust_next/manifest.rs`:

- manifest ingest loop from `TrustStore::add_from_manifest`
- trust rotation / revocation / expiry handling
- manifest cache expiry update

Moved to `crates/assay-registry/src/trust_next/cache.rs`:

- `needs_refresh`
- `clear_cached_keys`
- `empty_inner`

Moved to `crates/assay-registry/src/trust_next/access.rs`:

- `get_key_inner`
- `list_keys`
- `get_metadata`

Explicitly unchanged in this wave:

- `crates/assay-registry/tests/**`
- `crates/assay-registry/src/resolver.rs`
- `crates/assay-registry/src/verify.rs`
- `crates/assay-cli/**`
- `crates/assay-evidence/**`
