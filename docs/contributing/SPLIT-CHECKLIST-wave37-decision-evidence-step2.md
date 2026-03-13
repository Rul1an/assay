# SPLIT CHECKLIST - Wave37 Decision Evidence Convergence Step2

## Scope discipline
- [ ] Diff is limited to bounded runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside MCP runtime/test paths
- [ ] No new obligation types added
- [ ] No policy backend/control-plane/auth transport changes

## Implementation contract
- [ ] Canonical convergence outcome fields are additive:
  - `decision_outcome_kind`
  - `decision_origin`
  - `outcome_compat_state`
- [ ] Deterministic deny classification is explicit:
  - `policy_deny`
  - `fail_closed_deny`
  - `enforcement_deny`
- [ ] Deterministic obligation classification is explicit:
  - `obligation_applied`
  - `obligation_skipped`
  - `obligation_error`
- [ ] Existing fulfillment normalization remains intact:
  - `fulfillment_decision_path`
  - `obligation_applied_present`
  - `obligation_skipped_present`
  - `obligation_error_present`

## Compatibility and behavior
- [ ] Existing decision/event fields remain present and backward-compatible
- [ ] Existing execution behavior (`log`, `alert`, `approval_required`, `restrict_scope`, `redact_args`) remains intact
- [ ] `policy_deny` vs `fail_closed_deny` remains explicitly distinguishable
- [ ] No new runtime capability is introduced in this wave

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave37-decision-evidence-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
