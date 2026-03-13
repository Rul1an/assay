# SPLIT CHECKLIST - Wave38 Replay Diff Step2

## Scope discipline
- [ ] Diff is limited to bounded runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside replay/diff implementation paths
- [ ] No new runtime capability added
- [ ] No policy backend/control-plane/auth transport changes

## Implementation contract
- [ ] Typed replay basis is additive and present:
  - `ReplayDiffBasis`
  - `basis_from_decision_data`
- [ ] Typed replay diff buckets are additive and present:
  - `unchanged`
  - `stricter`
  - `looser`
  - `reclassified`
  - `evidence_only`
- [ ] Deterministic classifier is present:
  - `classify_replay_diff`

## Compatibility and behavior
- [ ] Existing decision/event fields remain backward-compatible
- [ ] Existing runtime enforcement behavior remains unchanged
- [ ] Replay classifier is deterministic for unchanged/strictness/reclassification

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave38-replay-diff-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
