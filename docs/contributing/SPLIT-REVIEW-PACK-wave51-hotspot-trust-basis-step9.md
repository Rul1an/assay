# SPLIT REVIEW PACK - Wave 51 Trust Basis Step9

## Summary

Step 9 reduces the Trust Basis hotspot by moving public types and diff helpers behind the existing `trust_basis` facade. It is intended to be a behavior-preserving module split only.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-evidence/src/trust_basis.rs` | 2214 | 1892 | -322 |
| `crates/assay-evidence/src/trust_basis/types.rs` | 0 | 160 | +160 |
| `crates/assay-evidence/src/trust_basis/diff.rs` | 0 | 180 | +180 |

## Boundary Proof

Facade delegates/re-exports:

```bash
rg -n 'mod diff;|mod types;|pub use diff::|pub use types::' crates/assay-evidence/src/trust_basis.rs
```

Types are no longer defined in the facade:

```bash
! rg -n 'pub enum TrustClaimId|pub struct TrustBasisClaim|pub struct TrustBasisDiffReport' crates/assay-evidence/src/trust_basis.rs
rg -n 'pub enum TrustClaimId|pub struct TrustBasisClaim|pub struct TrustBasisDiffReport' crates/assay-evidence/src/trust_basis/types.rs
```

Diff helpers are no longer defined in the facade:

```bash
! rg -n 'pub fn diff_trust_basis|pub fn duplicate_trust_basis_claim_ids|fn trust_claim_level_rank' crates/assay-evidence/src/trust_basis.rs
rg -n 'pub fn diff_trust_basis|pub fn duplicate_trust_basis_claim_ids|fn trust_claim_level_rank' crates/assay-evidence/src/trust_basis/diff.rs
```

Generation remains in the facade:

```bash
rg -n 'pub fn generate_trust_basis|pub fn to_canonical_json_bytes|fn classify_external_eval_receipt_boundary|fn classify_pack_findings' crates/assay-evidence/src/trust_basis.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-evidence`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib trust_basis_contract_`
- `cargo test -p assay-evidence --lib trust_basis`
- `bash scripts/ci/review-wave51-hotspot-trust-basis-step9.sh`

## Next Step

Step 10 should mechanically move generation and classifier internals into dedicated modules while keeping `generate_trust_basis` and `to_canonical_json_bytes` as facade entrypoints.
