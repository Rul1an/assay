# SPLIT CHECKLIST - Wave36 Redact Args Enforcement Step2

## Scope discipline
- [ ] Diff is limited to bounded runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside MCP runtime/test paths
- [ ] No new obligation types added
- [ ] No policy backend/control-plane/auth transport changes

## Implementation contract
- [ ] Runtime redaction is executed in the handler for `redact_args`
- [ ] Redaction runtime modes remain bounded:
  - `mask`
  - `hash`
  - `drop`
  - `partial`
- [ ] Redaction targets remain bounded:
  - `path`
  - `query`
  - `headers`
  - `body`
  - `metadata`
  - `args`
- [ ] Deterministic failure classes remain explicit:
  - `redaction_target_missing`
  - `redaction_mode_unsupported`
  - `redaction_scope_unsupported`
  - `redaction_apply_failed`

## Evidence and compatibility
- [ ] Redaction evidence fields remain additive and backward-compatible:
  - `redaction_target`
  - `redaction_mode`
  - `redaction_scope`
  - `redaction_applied_state`
  - `redaction_reason`
  - `redaction_failure_reason`
  - `redact_args_present`
  - `redact_args_target`
  - `redact_args_mode`
  - `redact_args_result`
  - `redact_args_reason`
- [ ] `obligation_outcomes` stays normalized with deterministic markers
- [ ] Existing `log`, `alert`, `approval_required`, and `restrict_scope` behavior remains stable

## Non-goals still enforced
- [ ] No PII detection engine
- [ ] No external DLP integration
- [ ] No broad/global redact semantics
- [ ] No UI/control-plane additions

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave36-redact-args-enforcement-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
