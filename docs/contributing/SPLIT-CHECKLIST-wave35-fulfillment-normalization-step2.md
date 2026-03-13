# SPLIT CHECKLIST - Wave35 Fulfillment Normalization Step2

## Scope discipline
- [ ] Diff is limited to bounded runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside MCP runtime/consumer paths
- [ ] No new obligation types added
- [ ] No policy backend/control-plane/auth transport changes

## Implementation contract
- [ ] Normalized fulfillment shape remains additive:
  - `obligation_type`
  - `status`
  - `reason`
  - `reason_code`
  - `enforcement_stage`
  - `normalization_version`
- [ ] Deterministic reason-code defaults exist for normalized outcomes:
  - `obligation_applied`
  - `obligation_skipped`
  - `obligation_error`
- [ ] Deterministic fulfillment path mapping is represented:
  - `policy_allow`
  - `policy_deny`
  - `fail_closed_deny`
  - `decision_error`
- [ ] Additive presence markers are emitted:
  - `obligation_applied_present`
  - `obligation_skipped_present`
  - `obligation_error_present`

## Compatibility and behavior
- [ ] Existing typed decisions remain unchanged
- [ ] Existing obligation execution (`log`, `alert`, `approval_required`, `restrict_scope`, `redact_args`) remains intact
- [ ] Existing event fields remain present and backward-compatible
- [ ] `policy_deny` and `fail_closed_deny` stay explicitly distinguishable

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave35-fulfillment-normalization-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
