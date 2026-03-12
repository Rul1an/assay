# SPLIT CHECKLIST — Wave31 Redact Args Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave31-redact-args-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave31-redact-args-step3.md`
  - `scripts/ci/review-wave31-redact-args-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave31 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded `redact_args` contract/evidence behavior

## Redact args invariants
- [ ] Redact shape markers remain present:
  - `redaction_target`
  - `redaction_mode`
  - `redaction_scope`
  - `redaction_applied_state`
  - `redaction_reason`
- [ ] Additive redact evidence markers remain present:
  - `redact_args_present`
  - `redact_args_target`
  - `redact_args_mode`
  - `redact_args_result`
  - `redact_args_reason`
- [ ] `redact_args` remains contract-only in this wave (no enforcement)

## Non-goals still enforced
- [ ] No runtime arg rewrite/mutation
- [ ] No `redact_args` deny semantics
- [ ] No broad/global scrub semantics
- [ ] No PII engine or external DLP integration
- [ ] No control-plane/auth transport changes

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
