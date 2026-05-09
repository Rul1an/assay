# SPLIT CHECKLIST - Wave 51 Trust Basis Step11

## Goal

Mechanically split canonical Trust Basis JSON serialization into a tiny implementation module while preserving the public `to_canonical_json_bytes` facade and frozen canonical output contracts.

## Scope

Included:

- `crates/assay-evidence/src/trust_basis.rs`
- `crates/assay-evidence/src/trust_basis/canonical.rs`
- Step 11 SPLIT artifacts and reviewer gate

Excluded:

- canonical JSON formatting changes
- claim ordering changes
- generation/classifier/diff behavior changes
- test relocation
- CLI, Trust Card, workflow, or generated-file changes

## Boundary Rules

- `trust_basis.rs` remains the public facade and delegates `to_canonical_json_bytes` to `canonical.rs`.
- `trust_basis/canonical.rs` owns `PrettyFormatter`, `Serializer`, `Serialize`, and the trailing newline behavior.
- Existing Step 8 canonical JSON contract remains the behavior guard.
- Step 9/10 ownership remains unchanged for types, diff, generation, and classifiers.

## Validation

```bash
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
bash scripts/ci/review-wave51-hotspot-trust-basis-step11.sh
```

## Reviewer Focus

- Confirm the canonical serializer body moved 1:1.
- Confirm the public facade path is unchanged.
- Confirm the canonical JSON shape contract still runs.
