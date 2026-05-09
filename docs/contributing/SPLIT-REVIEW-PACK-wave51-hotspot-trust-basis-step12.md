# SPLIT REVIEW PACK - Wave 51 Trust Basis Step12

## Summary

Step 12 moves Trust Basis unit tests into `trust_basis/tests.rs`. This is a cleanup-only layout PR after production logic was split across focused modules.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-evidence/src/trust_basis.rs` | 1319 | 32 | -1287 |
| `crates/assay-evidence/src/trust_basis/tests.rs` | 0 | 1281 | +1281 |

## Boundary Proof

Facade only declares the test module:

```bash
rg -n '^#\[cfg\(test\)\]|^mod tests;' crates/assay-evidence/src/trust_basis.rs
! rg -n '^mod tests \{' crates/assay-evidence/src/trust_basis.rs
```

Tests moved with contract names preserved:

```bash
rg -n 'trust_basis_contract_generated_claim_id_order_is_frozen|trust_basis_contract_canonical_json_shape_is_frozen|trust_basis_contract_diff_report_ordering_is_frozen' crates/assay-evidence/src/trust_basis/tests.rs
```

Production modules unchanged in ownership:

```bash
rg -n '^mod canonical;|^mod classifiers;|^mod diff;|^mod generation;|^mod types;' crates/assay-evidence/src/trust_basis.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-evidence`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib trust_basis_contract_`
- `cargo test -p assay-evidence --lib trust_basis`
- `bash scripts/ci/review-wave51-hotspot-trust-basis-step12.sh`

## Next Step

Stop Wave 51 Trust Basis work here. The production facade is thin, and tests are isolated in a dedicated module file.
