# Wave R1 Step1 Move Map - Production Roots

## Ownership Map

- `crates/assay-registry/assets/production-trust-roots.json`
  - Adds the embedded bootstrap rootset compiled into the binary
- `crates/assay-registry/src/trust.rs`
  - Adds parse/load helpers for embedded production roots
  - Adds sync construction path used by resolver bootstrap
  - Adds fail-closed validation for invalid/empty embedded roots
- `crates/assay-registry/src/resolver.rs`
  - Switches production resolver bootstrap from `TrustStore::new()` to `TrustStore::from_production_roots()`
  - Adds a small contract test proving the default path is non-empty
- `crates/assay-registry/tests/resolver_production_roots.rs`
  - Adds signed accept + untrusted reject integration tests against a mock registry

## Explicit Non-moves

- No cache-layer refactor
- No `RegistryClient` API changes
- No keys-manifest verification flow
- No CLI or workflow changes
