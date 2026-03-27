# Wave48 Registry Trust Step2 Review Pack

## Intent

Mechanically split `crates/assay-registry/src/trust.rs` behind a stable facade, without changing
trust-store semantics or resolver/verification-visible behavior.

## Scope

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

## Non-goals

- no workflow changes
- no edits under `crates/assay-registry/tests/**`
- no edits to `resolver.rs` or `verify.rs`
- no trust-store redesign or optimization wave
- no public registry contract changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step2.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo check -q -p assay-registry
cargo test -q -p assay-registry --lib 'trust::tests::test_with_production_roots_loads_embedded_roots' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_add_from_manifest' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_pinned_key_not_overwritten' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_needs_refresh' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_revoke_old_key' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_pinned_root_survives_revocation' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_key_expires_after_added' -- --exact
cargo test -q -p assay-registry --test resolver_production_roots resolver_accepts_signed_pack_with_embedded_production_root -- --exact
cargo test -q -p assay-registry --test resolver_production_roots resolver_rejects_signed_pack_with_untrusted_key_id -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to `trust.rs`, `trust_next/*`, and Step2 docs/script.
2. Confirm `crates/assay-registry/tests/**`, `resolver.rs`, and `verify.rs` remain untouched.
3. Confirm `trust.rs` is now a thin facade with stable `TrustStore` entrypoints and inline tests.
4. Confirm the move-map treats this as responsibility-based relocation, not redesign.
5. Confirm the reviewer script pins trust-store semantics plus resolver coupling tests.
