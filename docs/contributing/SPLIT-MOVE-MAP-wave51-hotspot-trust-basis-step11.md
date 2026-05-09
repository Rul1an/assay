# SPLIT MOVE MAP - Wave 51 Trust Basis Step11

Step 11 moves the final production helper body out of the Trust Basis facade.

## Moves

| Before | After | Notes |
| --- | --- | --- |
| `trust_basis.rs::to_canonical_json_bytes` body | `trust_basis/canonical.rs::to_canonical_json_bytes` | Pretty formatter, serializer call, and trailing newline preserved. |
| `serde::Serialize` facade import | `trust_basis/canonical.rs` | Serializer dependency now lives with canonical implementation. |

## Stayed Put

- Public `to_canonical_json_bytes` facade entrypoint
- Public `generate_trust_basis` facade entrypoint
- `types.rs`, `diff.rs`, `generation.rs`, `classifiers.rs`
- all Trust Basis tests
- crate root re-exports

## Follow-up

The production Trust Basis split can stop here. A future cleanup-only PR may relocate tests into module-focused test files, but that should be separate from production movement.
