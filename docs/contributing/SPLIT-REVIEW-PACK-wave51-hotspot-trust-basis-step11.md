# SPLIT REVIEW PACK - Wave 51 Trust Basis Step11

## Summary

Step 11 moves canonical Trust Basis JSON serialization into `trust_basis/canonical.rs`. The public `to_canonical_json_bytes` facade remains unchanged and the Step 8 canonical JSON contract continues to freeze the output shape.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-evidence/src/trust_basis.rs` | 1324 | 1319 | -5 |
| `crates/assay-evidence/src/trust_basis/canonical.rs` | 0 | 12 | +12 |

Facade non-test LOC: `35 -> 30`.

## Boundary Proof

Facade delegates canonical serialization:

```bash
rg -n 'mod canonical;|pub fn to_canonical_json_bytes|canonical::to_canonical_json_bytes' crates/assay-evidence/src/trust_basis.rs
```

Canonical module owns serializer implementation:

```bash
rg -n 'PrettyFormatter|Serializer::with_formatter|serialize\(&mut serializer\)|output\.push\(b' crates/assay-evidence/src/trust_basis/canonical.rs
```

Facade no longer owns serializer implementation:

```bash
! rg -n 'PrettyFormatter|Serializer::with_formatter|serde::Serialize|output\.push\(b' crates/assay-evidence/src/trust_basis.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-evidence`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib trust_basis_contract_`
- `cargo test -p assay-evidence --lib trust_basis`
- `bash scripts/ci/review-wave51-hotspot-trust-basis-step11.sh`

## Next Step

Stop production splitting for Trust Basis here unless reviewers request test-layout cleanup. The facade is now production-thin; remaining line weight is test code.
