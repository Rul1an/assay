# SPLIT CHECKLIST — Wave29 Restrict Scope Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step3.md`
  - `scripts/ci/review-wave29-restrict-scope-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave29 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded restrict_scope contract/evidence semantics

## Restrict scope invariants
- [ ] Restrict scope contract markers remain present:
  - `restrict_scope`
  - `scope_type`
  - `scope_value`
  - `scope_match_mode`
  - `scope_evaluation_state`
  - `scope_failure_reason`
- [ ] Restrict scope evidence markers remain present:
  - `restrict_scope_present`
  - `restrict_scope_target`
  - `restrict_scope_match`
  - `restrict_scope_reason`
- [ ] `restrict_scope_mismatch_does_not_deny` remains true
- [ ] `restrict_scope_match_sets_additive_fields` remains true

## Non-goals still enforced
- [ ] No restrict_scope runtime enforcement added
- [ ] No arg rewriting or filtering added
- [ ] No `redact_args` execution added
- [ ] No broad/global scope semantics added

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `legacy_warning -> log` compatibility remains present
- [ ] `obligation_outcomes` remains additive

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
