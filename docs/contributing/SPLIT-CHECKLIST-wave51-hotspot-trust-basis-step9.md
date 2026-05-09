# SPLIT CHECKLIST - Wave 51 Trust Basis Step9

## Goal

Mechanically split Trust Basis public types and diff helpers out of `crates/assay-evidence/src/trust_basis.rs` while preserving the existing `assay_evidence::trust_basis::*` API and root crate re-exports.

## Scope

Included:

- `crates/assay-evidence/src/trust_basis.rs`
- `crates/assay-evidence/src/trust_basis/types.rs`
- `crates/assay-evidence/src/trust_basis/diff.rs`
- Step 9 SPLIT artifacts and reviewer gate

Excluded:

- generation behavior changes
- classifier behavior changes
- canonical JSON format changes
- claim ordering changes
- CLI or workflow changes
- Trust Card behavior changes

## Boundary Rules

- `trust_basis.rs` remains the stable facade and owns bundle loading, lint integration, claim generation, canonical JSON serialization, classifiers, and tests.
- `trust_basis/types.rs` owns Trust Basis enums, claim structs, diff structs, `TrustBasisOptions`, and `TRUST_BASIS_DIFF_SCHEMA`.
- `trust_basis/diff.rs` owns `diff_trust_basis`, duplicate-claim detection, rank/sort helpers, and presence-diff construction.
- `crates/assay-evidence/src/lib.rs` root re-exports remain unchanged.
- Existing Step 8 contract tests stay green.

## Validation

```bash
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
bash scripts/ci/review-wave51-hotspot-trust-basis-step9.sh
```

## Reviewer Focus

- Confirm this is a mechanical move, not a semantics patch.
- Confirm public names are still reachable through `assay_evidence::trust_basis::*` and crate root re-exports.
- Confirm canonical JSON and diff report contracts are unchanged.
- Confirm Step 10 can split generation/classifiers separately without mixing concerns into this PR.
