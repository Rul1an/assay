# Wave48 Registry Trust Step3 Review Pack (Closure)

## Intent

Close the shipped Wave48 registry-trust split with docs/gates only and forbid post-Step2 redesign drift.

## Scope

- `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
- `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step3.md`
- `scripts/ci/review-wave48-registry-trust-step3.sh`

## Non-goals

- no workflow changes
- no changes under `crates/assay-registry/src/**`
- no changes under `crates/assay-registry/tests/**`
- no new module cuts
- no trust-store redesign
- no resolver, validation, cache, or verification-coupling drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave48-registry-trust-step3.sh
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
cargo test -q -p assay-registry --test resolver_production_roots resolver_rejects_signed_pack_with_untrusted_key_id -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step3 allowlist.
2. Confirm `crates/assay-registry/src/**` and `crates/assay-registry/tests/**` are frozen in this wave.
3. Confirm the plan records `#970` as shipped and bounds Step3 to closure only.
4. Confirm the move-map freezes the current module ownership and does not propose another split.
5. Confirm the reviewer script re-runs the pinned trust/resolver invariants.
