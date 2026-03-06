# Registry Client Step1 Review Pack (Freeze)

## Intent

Freeze Wave 11 scope for `registry_client` test decomposition before any mechanical move.

## Scope

- `docs/contributing/SPLIT-PLAN-registry-client-wave11.md`
- `docs/contributing/SPLIT-CHECKLIST-registry-client-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step1.md`
- `scripts/ci/review-registry-client-step1.sh`

## Non-goals

- no code moves
- no test behavior changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-registry-client-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo test -p assay-registry --tests
```

## Reviewer 60s scan

1. Confirm only Step1 docs/script changed.
2. Confirm `registry_client.rs` untouched.
3. Confirm split map + invariants are explicit in plan.
4. Run reviewer script and expect PASS.
