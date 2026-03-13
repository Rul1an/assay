# SPLIT CHECKLIST — Wave32 Redact Args Enforcement Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave32-redact-args-enforcement-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave32-redact-args-enforcement-step3.md`
  - `scripts/ci/review-wave32-redact-args-enforcement-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave32 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded `redact_args` enforcement semantics

## Redact enforcement invariants
- [ ] Runtime markers remain present:
  - `P_REDACT_ARGS`
  - `validate_redact_args`
- [ ] Failure reasons remain present:
  - `redaction_target_missing`
  - `redaction_mode_unsupported`
  - `redaction_scope_unsupported`
  - `redaction_apply_failed`
- [ ] Additive evidence markers remain present:
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

## Non-goals still enforced
- [ ] No broad/global redaction behavior added
- [ ] No PII detection engine added
- [ ] No external DLP integration added
- [ ] No approval/restrict_scope semantics expansion added
- [ ] No control-plane/auth transport work added

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present
- [ ] `obligation_outcomes` remains additive

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
