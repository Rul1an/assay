# Wave48 Registry Trust Step2 Checklist (Mechanical Split)

## Scope lock

- [ ] Only these files changed:
  - `crates/assay-registry/src/trust.rs`
  - `crates/assay-registry/src/trust_next/mod.rs`
  - `crates/assay-registry/src/trust_next/decode.rs`
  - `crates/assay-registry/src/trust_next/pinned.rs`
  - `crates/assay-registry/src/trust_next/manifest.rs`
  - `crates/assay-registry/src/trust_next/cache.rs`
  - `crates/assay-registry/src/trust_next/access.rs`
  - `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step2.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step2.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step2.md`
  - `scripts/ci/review-wave48-registry-trust-step2.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-registry/tests/**`
- [ ] No edits under `crates/assay-registry/src/resolver.rs`
- [ ] No edits under `crates/assay-registry/src/verify.rs`
- [ ] No verify / resolver / client / CLI / evidence scope leak

## Mechanical split contract

- [ ] `trust.rs` remains the stable facade entrypoint for `TrustStore` and `KeyMetadata`
- [ ] `trust_next/*` carries the moved implementation bodies
- [ ] No pinned-root loading drift
- [ ] No key-id / SPKI validation drift
- [ ] No revoked/expired-key handling drift
- [ ] No cache/refresh drift
- [ ] No resolver / verification coupling drift
- [ ] Inline tests remain in `trust.rs`

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step2.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -p assay-registry --all-targets -- -D warnings` passes
- [ ] Pinned trust/resolver invariants pass
