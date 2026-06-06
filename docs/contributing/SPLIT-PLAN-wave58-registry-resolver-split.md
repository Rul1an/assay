# Wave58 Registry Resolver Split Plan

Goal:
- Mechanically split `crates/assay-registry/src/resolver.rs` behind a stable facade.
- Keep resolver behavior, public API, registry fetch semantics, cache policy, BYOS behavior, and tests unchanged.
- Use the Wave57 resolver contracts as the safety rail before touching the resolver hotspot.

Baseline:
- `crates/assay-registry/src/resolver.rs`: 589 LOC on `origin/main` before Wave58.
- Wave57 added external resolver contracts for cache-first, pinned-digest cache mismatch/refetch, and `no_cache`.
- Inline resolver unit tests covered local/bundled paths, source display, and production-root bootstrap.

Split shape:
- `resolver.rs`: public facade and re-export only.
- `resolver_next/mod.rs`: public resolver types, config, constructors, public resolution entrypoints, prefetch, and accessors.
- `resolver_next/local.rs`: local-file resolution.
- `resolver_next/bundled.rs`: configured and standard bundled-pack resolution.
- `resolver_next/registry.rs`: registry fetch, cache-first path, pinned digest check, signature verification, and cache write.
- `resolver_next/byos.rs`: BYOS HTTP fetch and unsupported-scheme error.
- `resolver_next/tests.rs`: moved inline resolver unit tests.

Non-goals:
- No resolver-level `304 Not Modified` behavior change.
- No registry client, cache, trust, verify, lockfile, Cargo, or workflow changes.
- No edition migration, dependency changes, performance tuning, or broad cleanup.

Review posture:
- Review as a move-only refactor guarded by contract tests.
- Any behavior change belongs in a follow-up behavior/design PR, not this split.
