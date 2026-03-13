# SPLIT CHECKLIST — Wave32 Redact Args Enforcement Step2

## Scope discipline
- [ ] Only bounded runtime + tests + Step2 docs/gate files changed
- [ ] No `.github/workflows/*` changes
- [ ] No non-wave scope leaks

## Enforcement contract
- [ ] `redact_args` is runtime-enforced
- [ ] Missing/unsupported/not-applied redaction requirements deterministically deny
- [ ] Deny reason code is explicit (`P_REDACT_ARGS`)
- [ ] Failure reasons remain deterministic:
  - `redaction_target_missing`
  - `redaction_mode_unsupported`
  - `redaction_scope_unsupported`
  - `redaction_apply_failed`

## Evidence compatibility
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
- [ ] Existing event consumers remain compatible

## Non-goals still enforced
- [ ] No broad/global redaction semantics added
- [ ] No PII detection engine added
- [ ] No external DLP integration added
- [ ] No approval/restrict_scope semantics expansion added
- [ ] No control-plane/auth transport work added

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave32-redact-args-enforcement-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
