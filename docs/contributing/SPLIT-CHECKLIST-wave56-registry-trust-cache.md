# Wave56 Registry Trust/Cache Split Checklist

Scope:
- Thin `assay-registry` trust/cache facades by moving existing unit contract tests into the already-active `trust_next` and `cache_next` module trees.
- Preserve public API, trust decisions, cache IO behavior, error variants, metadata shape, and test assertions.
- Do not touch resolver behavior in this wave.

Pre-move baseline:
- `crates/assay-registry/src/trust.rs`: 599 LOC on `origin/main`.
- `crates/assay-registry/src/cache.rs`: 592 LOC on `origin/main`.
- `trust_next` and `cache_next` already own runtime helper implementations; this wave moves tests to those boundaries.

Required checks:
- Facades keep public types and methods in place.
- `trust_next/mod.rs` and `cache_next/mod.rs` include tests only behind `#[cfg(test)]`.
- `trust_next/tests.rs` contains trust-store, production-root, key-id, and rotation contract tests.
- `cache_next/tests.rs` contains cache roundtrip, expiry, integrity, signature, metadata, atomic-write, and registry-url contract tests.
- No `resolver.rs`, `Cargo.toml`, `Cargo.lock`, workflow, runner, or eBPF drift.
- `BASE_REF=origin/main bash scripts/ci/review-wave56-registry-trust-cache.sh` passes.

Non-goals:
- No resolver split.
- No cache policy or trust decision changes.
- No edition 2024 migration.
- No dependency changes.
- No broad registry cleanup.
