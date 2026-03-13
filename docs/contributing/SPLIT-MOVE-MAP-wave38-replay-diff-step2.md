# SPLIT MOVE MAP - Wave38 Replay Diff Step2

## Intent
Bounded implementation for typed replay basis + deterministic diff buckets, with additive evidence comparison contract only.

## Touched runtime paths
- `crates/assay-core/src/mcp/decision/replay_diff.rs`
  - introduces:
    - `ReplayDiffBasis`
    - `ReplayDiffBucket`
    - `basis_from_decision_data`
    - `classify_replay_diff`
- `crates/assay-core/src/mcp/decision.rs`
  - exports replay/diff contract APIs from decision module

## Touched tests
- `crates/assay-core/tests/replay_diff_contract.rs`
  - validates deterministic buckets:
    - unchanged
    - stricter
    - looser
    - reclassified
    - evidence_only

## Out-of-scope guarantees
- no new obligation types
- no runtime enforcement expansion
- no policy backend/control-plane additions
- no auth transport changes
