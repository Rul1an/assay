# Wave R1 Step1 Inventory - Production Roots

## Intent

Finish the rooted-trust bootstrap for `assay-registry` without widening into Sigstore, manifest-rotation, or broader registry refactors.

## Scope Freeze

- `crates/assay-registry/assets/production-trust-roots.json`
- `crates/assay-registry/src/trust.rs`
- `crates/assay-registry/src/resolver.rs`
- `crates/assay-registry/tests/resolver_production_roots.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-r1-production-roots-step1.md`
- `scripts/ci/review-wave-r1-production-roots-step1.sh`

## Baseline

- `origin/main` HEAD at freeze: `2d3d504f16c934d3a5ec719e7cabc9f615009ebd`
- Hotspot: `crates/assay-registry/src/trust.rs` = 664 LOC
- Hotspot: `crates/assay-registry/src/resolver.rs` = 581 LOC
- Gap at freeze:
  - `TrustStore::with_production_roots()` returned an empty store
  - `PackResolver::with_config()` silently used `TrustStore::new()`
  - No accept/reject integration tests existed for the production-root path

## Acceptance Targets

- Embedded production roots load non-empty and pin successfully
- Resolver default production path uses embedded roots instead of an empty trust store
- Signed fixture using the embedded root resolves
- Untrusted key id is rejected hard
- Invalid or empty production root configuration fails closed

## Non-goals

- No Sigstore/Rekor
- No signed `/keys` manifest flow
- No new transports
- No dependency churn
- No workflow changes
