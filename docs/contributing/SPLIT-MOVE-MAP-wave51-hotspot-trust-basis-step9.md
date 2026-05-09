# SPLIT MOVE MAP - Wave 51 Trust Basis Step9

Step 9 performs the first mechanical Trust Basis implementation split after the Step 8 behavior freeze.

## Moves

| Before | After | Notes |
| --- | --- | --- |
| `trust_basis.rs` claim enums and claim structs | `trust_basis/types.rs` | Derives, serde rename attributes, field order, and comments preserved. |
| `trust_basis.rs` diff structs and report methods | `trust_basis/types.rs` | Struct field order and `has_changes`/`has_regressions` behavior preserved. |
| `trust_basis.rs` `TrustBasisOptions` | `trust_basis/types.rs` | Keeps lint options as an internal type dependency only. |
| `trust_basis.rs` diff and duplicate helpers | `trust_basis/diff.rs` | Stable public functions re-exported through the facade. |
| direct public definitions in `trust_basis.rs` | `pub use types::*` / `pub use diff::*` | Keeps public module path stable. |

## Stayed Put

- `generate_trust_basis`
- `to_canonical_json_bytes`
- signing/provenance/delegation/auth/degradation classifiers
- external receipt boundary classifiers
- pack finding classifier
- all Trust Basis tests

## Follow-up

Step 10 should split generation and classifiers into implementation modules after keeping the Step 8/9 contracts green.
