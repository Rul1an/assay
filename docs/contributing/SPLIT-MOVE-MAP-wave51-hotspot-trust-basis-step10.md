# SPLIT MOVE MAP - Wave 51 Trust Basis Step10

Step 10 performs the second mechanical Trust Basis implementation split after Step 9 moved types and diff helpers.

## Moves

| Before | After | Notes |
| --- | --- | --- |
| `trust_basis.rs::generate_trust_basis` body | `trust_basis/generation.rs::generate_trust_basis` | Public facade keeps the same function name and delegates. |
| `trust_basis.rs` signing/provenance/delegation/auth/degradation classifiers | `trust_basis/classifiers.rs` | Function bodies preserved; visibility narrowed to `pub(super)` only where generation needs access. |
| `trust_basis.rs` external receipt constants and guards | `trust_basis/classifiers.rs` | Promptfoo, OpenFeature, and CycloneDX ML-BOM receipt validation logic preserved. |
| `trust_basis.rs` bounded string/digest/time helpers | `trust_basis/classifiers.rs` | Private helper functions stay private to classifier module. |
| `trust_basis.rs::classify_pack_findings` | `trust_basis/classifiers.rs` | Visibility narrowed to `pub(super)` for generation. |

## Stayed Put

- Public `generate_trust_basis` facade entrypoint
- Public `to_canonical_json_bytes` facade entrypoint
- `trust_basis/types.rs`
- `trust_basis/diff.rs`
- all Trust Basis tests
- crate root re-exports

## Follow-up

A later cleanup step can decide whether tests should move into focused module-level test files. This step intentionally leaves tests in the facade to avoid mixing test relocation with behavior-preserving production moves.
