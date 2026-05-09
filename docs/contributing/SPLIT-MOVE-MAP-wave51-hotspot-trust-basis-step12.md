# SPLIT MOVE MAP - Wave 51 Trust Basis Step12

Step 12 is a cleanup-only test-layout move after the Trust Basis production split was completed.

## Moves

| Before | After | Notes |
| --- | --- | --- |
| inline `#[cfg(test)] mod tests { ... }` in `trust_basis.rs` | `trust_basis/tests.rs` | Test bodies, helper builders, and assertions preserved. |
| large test body in facade | `#[cfg(test)] mod tests;` | Facade remains production-thin and delegates test module loading. |

## Stayed Put

- `trust_basis/canonical.rs`
- `trust_basis/classifiers.rs`
- `trust_basis/diff.rs`
- `trust_basis/generation.rs`
- `trust_basis/types.rs`
- public facade functions and re-exports

## Follow-up

No further Trust Basis hotspot split is planned after this. Future work should be feature-driven rather than mechanical splitting.
