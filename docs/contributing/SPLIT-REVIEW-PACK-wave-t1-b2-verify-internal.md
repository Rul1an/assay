# Wave T1 B2 Review Pack - verify_internal tests Mechanical Split

## Intent

Reduce review load by decomposing `verify_internal/tests.rs` into cohesive modules without changing verification behavior contracts.

## Scope

- `crates/assay-registry/src/verify_internal/tests.rs` (removed)
- `crates/assay-registry/src/verify_internal/tests/*` (added)
- `docs/contributing/SPLIT-CHECKLIST-wave-t1-b2-verify-internal.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-t1-b2-verify-internal.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b2-verify-internal.md`
- `scripts/ci/review-wave-t1-b2-verify-internal.sh`

## Non-goals

- No test semantic updates.
- No production verify logic changes.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<previous-step-commit> bash scripts/ci/review-wave-t1-b2-verify-internal.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-registry --tests -- -D warnings
cargo test -p assay-registry verify_internal
```

## Reviewer 60s Scan

1. Confirm one-to-one section moves from legacy `tests.rs` into split modules.
2. Confirm helper wrappers remain centralized in `tests/mod.rs`.
3. Confirm fail-closed/canonicalization contract tests still present.
4. Run reviewer script and check PASS.
