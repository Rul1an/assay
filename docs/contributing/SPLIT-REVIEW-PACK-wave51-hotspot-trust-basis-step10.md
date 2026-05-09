# SPLIT REVIEW PACK - Wave 51 Trust Basis Step10

## Summary

Step 10 moves Trust Basis generation and classifier internals behind the existing public facade. `trust_basis.rs` now keeps the public entrypoints and tests, while production implementation details live in focused `generation` and `classifiers` modules.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-evidence/src/trust_basis.rs` | 1893 | 1324 | -569 |
| `crates/assay-evidence/src/trust_basis/generation.rs` | 0 | 117 | +117 |
| `crates/assay-evidence/src/trust_basis/classifiers.rs` | 0 | 485 | +485 |

Facade non-test LOC: `607 -> 35`.

## Boundary Proof

Facade declares modules and delegates generation:

```bash
rg -n 'mod classifiers;|mod generation;|pub fn generate_trust_basis|generation::generate_trust_basis|pub fn to_canonical_json_bytes' crates/assay-evidence/src/trust_basis.rs
```

Generation owns bundle loading/lint/claim vector construction:

```bash
rg -n 'BundleReader::open_with_limits|lint_bundle_with_options|claims: vec!' crates/assay-evidence/src/trust_basis/generation.rs
```

Classifiers own receipt guards and pack finding classification:

```bash
rg -n 'PROMPTFOO_RECEIPT_EVENT_TYPE|OPENFEATURE_DECISION_RECEIPT_EVENT_TYPE|CYCLONEDX_MLBOM_MODEL_RECEIPT_EVENT_TYPE|classify_pack_findings' crates/assay-evidence/src/trust_basis/classifiers.rs
```

Facade no longer owns classifier internals:

```bash
! rg -n '^fn classify_|^const PROMPTFOO_|^const OPENFEATURE_|^const CYCLONEDX_|^fn is_supported_' crates/assay-evidence/src/trust_basis.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-evidence`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo test -p assay-evidence --lib trust_basis_contract_`
- `cargo test -p assay-evidence --lib trust_basis`
- `bash scripts/ci/review-wave51-hotspot-trust-basis-step10.sh`

## Next Step

Step 11 can either stop Trust Basis production splitting here and open a cleanup/test-layout PR, or move canonical JSON into a tiny `canonical.rs` module if reviewers prefer an even thinner facade.
