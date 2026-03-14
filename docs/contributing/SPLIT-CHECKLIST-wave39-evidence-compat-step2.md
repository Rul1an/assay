# SPLIT CHECKLIST - Wave39 Evidence Compat Step2

## Scope discipline
- [ ] Diff is limited to bounded replay/evidence runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside replay/evidence compatibility paths
- [ ] No new runtime capability added
- [ ] No policy backend/control-plane/auth transport changes

## Implementation contract
- [ ] Additive compatibility fields are present:
  - `decision_basis_version`
  - `compat_fallback_applied`
  - `classification_source`
  - `replay_diff_reason`
  - `legacy_shape_detected`
- [ ] Deterministic classification precedence is represented:
  - converged markers
  - fulfillment-path fallback
  - legacy fallback
- [ ] Legacy shape detection/fallback remains backward-compatible

## Compatibility and behavior
- [ ] Existing decision/event fields remain backward-compatible
- [ ] Existing runtime enforcement behavior remains unchanged
- [ ] Replay diff classifier remains deterministic for unchanged/strictness/reclassification

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave39-evidence-compat-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
