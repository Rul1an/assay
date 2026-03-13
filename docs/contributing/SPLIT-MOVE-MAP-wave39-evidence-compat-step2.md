# SPLIT MOVE MAP - Wave39 Evidence Compat Step2

## Intent
Bounded implementation for replay/evidence compatibility normalization fields and deterministic precedence, additive only.

## Touched runtime paths
- `crates/assay-core/src/mcp/decision.rs`
  - adds additive Decision Event fields:
    - `decision_basis_version`
    - `compat_fallback_applied`
    - `classification_source`
    - `replay_diff_reason`
    - `legacy_shape_detected`
  - populates fields via deterministic compatibility projection in normalization flow
- `crates/assay-core/src/mcp/decision/replay_compat.rs`
  - introduces deterministic compatibility projection and precedence helpers
  - exposes `ReplayClassificationSource` + `DECISION_BASIS_VERSION_V1`
- `crates/assay-core/src/mcp/decision/replay_diff.rs`
  - extends `ReplayDiffBasis` with compatibility normalization fields
  - derives deterministic fallback metadata from decision payloads

## Touched tests
- `crates/assay-core/tests/replay_diff_contract.rs`
  - validates additive basis fields
  - validates precedence fallback behavior for fulfillment-path and legacy shapes
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - validates emitted events include deterministic Wave39 compatibility markers

## Out-of-scope guarantees
- no runtime behavior change
- no new obligation types
- no policy backend/control-plane/auth transport scope
- no workflow changes
