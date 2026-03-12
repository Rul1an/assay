# SPLIT CHECKLIST — Wave31 Redact Args Step2

## Scope discipline
- [ ] Only bounded runtime + tests + Step2 docs/gate files changed
- [ ] No `.github/workflows/*` changes
- [ ] No non-wave scope leaks

## Contract/evidence implementation
- [ ] Typed `redact_args` obligation shape is represented in code
- [ ] Redactable zones are represented as contract data
- [ ] Additive evidence fields are present:
  - `redaction_target`
  - `redaction_mode`
  - `redaction_scope`
  - `redaction_applied_state`
  - `redaction_reason`
  - `redact_args_present`
  - `redact_args_target`
  - `redact_args_mode`
  - `redact_args_result`
  - `redact_args_reason`
- [ ] Existing decision/event consumers remain compatible

## Non-goals still enforced
- [ ] No runtime payload mutation/rewrite for args
- [ ] No runtime `redact_args` enforcement/deny path
- [ ] No broad/global scrub semantics
- [ ] No PII engine/DLP integrations
- [ ] No control-plane/auth transport changes

## Existing obligation line remains stable
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave31-redact-args-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
