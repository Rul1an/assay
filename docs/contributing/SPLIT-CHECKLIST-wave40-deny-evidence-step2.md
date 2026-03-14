# SPLIT CHECKLIST - Wave40 Deny Evidence Step2

## Scope discipline
- [ ] Diff is limited to bounded deny-evidence convergence runtime + tests + Step2 docs/gate.
- [ ] No `.github/workflows/*` changes.
- [ ] No scope leaks outside decision/evidence compatibility paths.
- [ ] No new runtime capability added.
- [ ] No policy backend/control-plane/auth transport changes.

## Implementation contract
- [ ] Additive deny-convergence fields are present:
  - `policy_deny`
  - `fail_closed_deny`
  - `enforcement_deny`
  - `deny_precedence_version`
  - `deny_classification_source`
  - `deny_legacy_fallback_applied`
  - `deny_convergence_reason`
- [ ] Deterministic deny precedence is represented and test-covered:
  - `decision_outcome_kind`
  - `decision_origin`
  - `fulfillment_decision_path`
  - legacy decision fallback
- [ ] Legacy deny fallback remains additive and backward-compatible.

## Compatibility and behavior
- [ ] Existing decision/event fields remain backward-compatible.
- [ ] Existing runtime enforcement behavior remains unchanged.
- [ ] Replay diff classifier remains deterministic for unchanged/strictness/reclassification.

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave40-deny-evidence-step2.sh` passes.
- [ ] `cargo fmt --check` passes.
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes.
- [ ] Pinned runtime/event tests pass.
