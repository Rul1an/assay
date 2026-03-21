# Wave R1 Step1 Review Pack - Production Roots

## Intent

Close the silent-empty trust bootstrap in `assay-registry` by embedding a real rootset, wiring the resolver production path to it, and proving accept/reject behavior with targeted tests.

## Scope

- `crates/assay-registry/assets/production-trust-roots.json`
- `crates/assay-registry/src/trust.rs`
- `crates/assay-registry/src/resolver.rs`
- `crates/assay-registry/tests/resolver_production_roots.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-r1-production-roots-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-r1-production-roots-step1.md`
- `scripts/ci/review-wave-r1-production-roots-step1.sh`

## Non-goals

- No Sigstore/Rekor
- No `/keys` manifest signing flow
- No registry transport expansion
- No dependency bump wave

## Validation Command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave-r1-production-roots-step1.sh
```

This gate runs:

```bash
cargo fmt --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo test -q -p assay-registry
git diff --check
```

## Reviewer 60s Scan

1. Confirm only the allowlisted registry trust files and review artifacts changed.
2. Confirm `resolver.rs` now uses `TrustStore::from_production_roots()` and no longer `TrustStore::new()` in the production path.
3. Confirm `trust.rs` rejects empty or invalid embedded roots.
4. Run the reviewer script and verify the signed accept + untrusted reject tests pass.
