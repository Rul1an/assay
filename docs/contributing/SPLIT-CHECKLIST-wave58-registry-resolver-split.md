# Wave58 Registry Resolver Split Checklist

Scope:
- [x] `resolver.rs` remains the stable public facade.
- [x] Public API stays re-exported as `PackResolver`, `ResolvedPack`, `ResolveSource`, and `ResolverConfig`.
- [x] Private route logic moves into `resolver_next/*` modules.
- [x] Inline resolver tests move with the implementation into `resolver_next/tests.rs`.
- [x] Wave57 external resolver contracts remain unchanged and green.

Behavior freeze:
- [x] No registry fetch semantics changed.
- [x] No cache lookup, pinned-digest, eviction, or cache-write policy changed.
- [x] No signature verification policy changed.
- [x] No BYOS scheme support changed.
- [x] No local or bundled resolution paths changed.

Validation:
- [x] `cargo fmt --check`
- [x] `cargo check -p assay-registry`
- [x] `cargo test -p assay-registry resolver`
- [x] `cargo test -p assay-registry --test resolver_contracts`
- [x] `cargo clippy -p assay-registry --all-targets -- -D warnings`
- [x] `BASE_REF=origin/main bash scripts/ci/review-wave58-registry-resolver-split.sh`

Review notes:
- `resolver_next` is intentionally nested under the `resolver` module via `#[path = "resolver_next/mod.rs"]` so child modules can use existing private resolver fields without widening visibility.
- `resolver_level 304` remains explicitly deferred from Wave57.
