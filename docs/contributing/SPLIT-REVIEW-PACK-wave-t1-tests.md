# Wave T1 Step A Review Pack - Test Hardening Freeze

## Intent

Freeze test-hardening scope and lock reviewer gates before any mechanical file split.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave-t1-tests.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-t1-tests.md`
- `scripts/ci/review-wave-t1-tests-a.sh`

## Non-goals

- No test logic moves yet.
- No assertion/fixture/exit-code behavior changes.
- No workflow changes.

## Hotspots Frozen in This Step

- `crates/assay-core/tests/parity.rs`
- `crates/assay-registry/src/verify_internal/tests.rs`
- `crates/assay-cli/tests/contract_exit_codes.rs`

## Next Mechanical Steps (Frozen Layout Intent)

1. `parity.rs` split into `tests/parity/*` behind a stable `mod.rs` harness.
2. `verify_internal/tests.rs` split into `verify_internal/tests/*` grouped by digest/dsse/provenance/failures.
3. `contract_exit_codes.rs` split into `tests/exit_codes/*` grouped by core/replay and additional domains if inventory confirms active coverage.

## Validation Command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave-t1-tests-a.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core -p assay-registry -p assay-cli --all-targets -- -D warnings
cargo test -p assay-core --test parity
cargo test -p assay-registry verify_internal
cargo test -p assay-cli --test contract_exit_codes
```

## Reviewer 60s Scan

1. Verify only Step A docs/script changed.
2. Verify allowlist-only and workflow-ban gates are hard-fail.
3. Verify Step A blocks edits to the three target test files.
4. Run reviewer script and confirm PASS from `origin/main`.
