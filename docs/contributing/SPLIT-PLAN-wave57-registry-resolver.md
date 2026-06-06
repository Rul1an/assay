# Wave57 Registry Resolver Contract Plan

Goal:
- Add resolver characterization tests before any resolver split.
- Keep `resolver.rs` behavior and public API unchanged in this step.
- Make the later `resolver_next` split reviewable by pinning cache, pinned-digest, and `no_cache` contracts first.

Observed baseline:
- `crates/assay-registry/src/resolver.rs`: 589 LOC on `origin/main`.
- Existing inline resolver tests cover local/bundled paths, source display, and production-root bootstrap.
- Existing external resolver tests cover production-root signature accept/reject.
- Registry-client tests cover HTTP fetch/304 behavior at the client layer, not resolver closure behavior.

Contracts added in this step:
- Fresh cache entries win before network fetch.
- Pinned digest mismatch in cache evicts the stale entry and refetches from registry.
- `no_cache` skips a cached entry and fetches from registry.

Explicit deferred gap:
- Resolver-level `304 Not Modified` behavior is not claimed as a green contract in this step. Current control flow returns a valid cache entry before network revalidation, while an unusable cache entry cannot safely satisfy the later `304` branch. Treat that as a future behavior/design PR, not as part of a mechanical split.

Next split prerequisites:
- Keep these contract tests green.
- Add a move-map for `resolver_next` only after deciding module boundaries for local/bundled, registry/cache, BYOS, and prefetch.
- Do not combine resolver behavior changes with the mechanical split.
