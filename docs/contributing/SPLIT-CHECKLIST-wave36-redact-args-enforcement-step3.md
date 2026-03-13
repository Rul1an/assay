# SPLIT CHECKLIST - Wave36 Redact Args Enforcement Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave36-redact-args-enforcement-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave36-redact-args-enforcement-step3.md`
  - `scripts/ci/review-wave36-redact-args-enforcement-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave36 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded runtime `redact_args` execution semantics

## Redaction enforcement invariants
- [ ] Runtime execution markers remain present:
  - `effective_arguments`
  - `validate_redact_args`
  - `apply_redact_args_runtime`
  - `redaction_target_value_mut`
  - `apply_value_redaction`
- [ ] Deterministic failure classes remain present:
  - `redaction_target_missing`
  - `redaction_mode_unsupported`
  - `redaction_scope_unsupported`
  - `redaction_apply_failed`
  - `P_REDACT_ARGS`
- [ ] Additive redaction evidence markers remain present:
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
  - `obligation_outcomes`

## Non-goals still enforced
- [ ] No PII detection engine
- [ ] No external DLP integration
- [ ] No broad/global redact semantics
- [ ] No UI/control-plane additions

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present
- [ ] `legacy_warning -> log` compatibility remains present

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
