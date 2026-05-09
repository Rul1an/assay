# SPLIT REVIEW PACK - Wave 51 Trust Basis Step8

## Summary

Step 8 starts the Trust Basis hotspot work with behavior-freeze contracts only. It adds tests that pin generated claim order, canonical JSON shape, and diff report ordering before any code is moved out of `trust_basis.rs`.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-evidence/src/trust_basis.rs` | 2012 | 2214 | +202 |

## Boundary Proof

No implementation split yet:

```bash
! test -d crates/assay-evidence/src/trust_basis
```

Trust Basis public entrypoints remain in the same file:

```bash
rg -n 'pub fn diff_trust_basis|pub fn generate_trust_basis|pub fn to_canonical_json_bytes|pub fn duplicate_trust_basis_claim_ids' crates/assay-evidence/src/trust_basis.rs
```

Freeze contracts exist:

```bash
rg -n 'trust_basis_contract_generated_claim_id_order_is_frozen|trust_basis_contract_canonical_json_shape_is_frozen|trust_basis_contract_diff_report_ordering_is_frozen' crates/assay-evidence/src/trust_basis.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-evidence`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib trust_basis_contract_`
- `cargo test -p assay-evidence --lib trust_basis`
- `bash scripts/ci/review-wave51-hotspot-trust-basis-step8.sh`

## Next Step

Step 9 should mechanically split Trust Basis types and diff helpers first, preserving root crate re-exports and the frozen contract tests added here.
