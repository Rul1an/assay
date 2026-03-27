# Wave48 Registry Trust Step1 Review Pack

## Intent

Freeze the split boundaries for `crates/assay-registry/src/trust.rs` before any mechanical module
moves, while preserving pinned-root, manifest, cache, and verification semantics.

## Scope

- `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
- `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step1.md`
- `scripts/ci/review-wave48-registry-trust-step1.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- no verify / resolver / client / CLI / evidence changes
- no trust-store redesign, optimization, or test reorganization

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo test -q -p assay-registry --lib 'trust::tests::test_with_production_roots_loads_embedded_roots' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_add_from_manifest' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_pinned_key_not_overwritten' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_needs_refresh' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_revoke_old_key' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_pinned_root_survives_revocation' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_key_expires_after_added' -- --exact
cargo test -q -p assay-registry --test resolver_production_roots resolver_accepts_signed_pack_with_embedded_production_root -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the 5 Step1 files.
2. Confirm `crates/assay-registry/src/**` and `crates/assay-registry/tests/**` remain untouched.
3. Confirm the plan freezes `TrustStore` as the stable facade.
4. Confirm the move-map previews module cuts by trust responsibility, not redesign.
5. Confirm the reviewer script re-runs pinned trust/resolver invariants.
