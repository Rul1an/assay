# SPLIT CHECKLIST - Wave 51 Trust Basis Step12

## Goal

Move the large Trust Basis unit-test module out of `crates/assay-evidence/src/trust_basis.rs` into `crates/assay-evidence/src/trust_basis/tests.rs` without changing production code or test behavior.

## Scope

Included:

- `crates/assay-evidence/src/trust_basis.rs`
- `crates/assay-evidence/src/trust_basis/tests.rs`
- Step 12 SPLIT artifacts and reviewer gate

Excluded:

- production Trust Basis behavior changes
- test assertion rewrites
- canonical JSON, generation, classifier, diff, or type changes
- CLI, Trust Card, workflow, or generated-file changes

## Boundary Rules

- `trust_basis.rs` keeps only `#[cfg(test)] mod tests;` for tests.
- `trust_basis/tests.rs` owns all Trust Basis unit tests and helper builders.
- Existing test names and contract-test prefixes remain unchanged.
- Production modules from Steps 9-11 remain unchanged.

## Validation

```bash
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
bash scripts/ci/review-wave51-hotspot-trust-basis-step12.sh
```

## Reviewer Focus

- Confirm this is a test-layout move only.
- Confirm the Trust Basis facade is production-thin.
- Confirm contract tests still run under the same names.
