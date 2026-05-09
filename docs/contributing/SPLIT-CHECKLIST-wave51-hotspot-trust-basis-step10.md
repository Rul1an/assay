# SPLIT CHECKLIST - Wave 51 Trust Basis Step10

## Goal

Mechanically split Trust Basis generation and classifier internals out of `crates/assay-evidence/src/trust_basis.rs` while keeping the public facade entrypoints stable.

## Scope

Included:

- `crates/assay-evidence/src/trust_basis.rs`
- `crates/assay-evidence/src/trust_basis/generation.rs`
- `crates/assay-evidence/src/trust_basis/classifiers.rs`
- Step 10 SPLIT artifacts and reviewer gate

Excluded:

- claim ordering changes
- classifier semantics changes
- external receipt schema/field allowlist changes
- canonical JSON format changes
- diff behavior changes
- CLI, Trust Card, workflow, or generated-file changes

## Boundary Rules

- `trust_basis.rs` remains the stable facade and keeps public `generate_trust_basis` and `to_canonical_json_bytes` entrypoints.
- `trust_basis/generation.rs` owns bundle byte limiting, `BundleReader` opening, lint pack integration, and frozen claim vector construction.
- `trust_basis/classifiers.rs` owns signing/provenance/delegation/auth/degradation classifiers, external receipt boundary guards, bounded-string validators, and pack finding classification.
- `trust_basis/types.rs` and `trust_basis/diff.rs` remain the Step 9 owners for types and diff helpers.
- Existing Step 8 contract tests stay green.

## Validation

```bash
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
bash scripts/ci/review-wave51-hotspot-trust-basis-step10.sh
```

## Reviewer Focus

- Confirm `trust_basis.rs` is now a thin facade plus existing tests.
- Confirm generation/classifier code was moved mechanically, not rewritten.
- Confirm receipt guard constants and allowlists moved intact.
- Confirm public re-exports in `crates/assay-evidence/src/lib.rs` remain unchanged.
