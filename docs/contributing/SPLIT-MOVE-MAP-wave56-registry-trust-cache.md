# Wave56 Registry Trust/Cache Move Map

Mechanical moves:

| Before | After | Contract |
| --- | --- | --- |
| `crates/assay-registry/src/trust.rs` inline `#[cfg(test)] mod tests` | `crates/assay-registry/src/trust_next/tests.rs` | Same test bodies and assertions; module is gated from `trust_next/mod.rs`. |
| `crates/assay-registry/src/cache.rs` inline `#[cfg(test)] mod tests` | `crates/assay-registry/src/cache_next/tests.rs` | Same test bodies and assertions; module is gated from `cache_next/mod.rs`. |

Facade ownership after move:
- `trust.rs` keeps `TrustStore`, `TrustStoreInner`, `KeyMetadata`, constants, public constructors, and public methods.
- `cache.rs` keeps `CacheMeta`, `PackCache`, `CacheEntry`, constants, public constructors, and public methods.

Implementation ownership after move:
- `trust_next/access.rs`: key lookup, metadata lookup, key list.
- `trust_next/cache.rs`: trust-store empty state, refresh status, cached-key clearing.
- `trust_next/manifest.rs`: manifest ingestion and rotation semantics.
- `trust_next/pinned.rs`: pinned-root parsing/loading/preparation.
- `cache_next/read.rs`: cache read/list/metadata path.
- `cache_next/put.rs`: cache write path.
- `cache_next/evict.rs`: eviction and clear path.
- `cache_next/policy.rs`: TTL parsing.
- `cache_next/io.rs`: cache directory and atomic write helper.

Explicitly unchanged:
- Public exports from `crates/assay-registry/src/lib.rs`.
- Resolver production-root behavior.
- Digest, DSSE, and signature verification code.
- Cache metadata JSON fields and stored filenames.
