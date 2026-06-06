# Wave58 Registry Resolver Move Map

| Before | After | Notes |
| --- | --- | --- |
| `resolver.rs` module docs | `resolver.rs` | Kept in facade. |
| `ResolvedPack` | `resolver_next/mod.rs` | Re-exported by `resolver.rs`. |
| `ResolveSource` + `Display` | `resolver_next/mod.rs` | Re-exported by `resolver.rs`; display output unchanged. |
| `ResolverConfig` + builder methods | `resolver_next/mod.rs` | Re-exported by `resolver.rs`. |
| `PackResolver` struct + constructors | `resolver_next/mod.rs` | Re-exported by `resolver.rs`; fields remain private. |
| `resolve` / `resolve_ref` | `resolver_next/mod.rs` | Public entrypoints remain on `PackResolver`. |
| `resolve_local` | `resolver_next/local.rs` | Moved 1:1. |
| `resolve_bundled` | `resolver_next/bundled.rs` | Moved 1:1. |
| `resolve_registry` / `try_cache` | `resolver_next/registry.rs` | Moved 1:1. |
| `resolve_byos` | `resolver_next/byos.rs` | Moved 1:1. |
| `prefetch` / `cache` / `trust_store` | `resolver_next/mod.rs` | Public methods remain on `PackResolver`. |
| Inline `#[cfg(test)] mod tests` | `resolver_next/tests.rs` | Moved 1:1 with imports adjusted. |

LOC delta:
- `crates/assay-registry/src/resolver.rs`: 589 -> 13.
- New `resolver_next/mod.rs`: 217.
- New `resolver_next/registry.rs`: 154.
- New `resolver_next/tests.rs`: 92.
- New `resolver_next/bundled.rs`: 70.
- New `resolver_next/byos.rs`: 48.
- New `resolver_next/local.rs`: 39.

Deferred:
- No `resolver_next` behavior cleanup.
- No resolver-level 304 contract addition.
- No registry client/cache/trust follow-up in this PR.
