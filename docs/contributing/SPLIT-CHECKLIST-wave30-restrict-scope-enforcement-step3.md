# SPLIT CHECKLIST — Wave30 Restrict Scope Enforcement Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave30-restrict-scope-enforcement-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave30-restrict-scope-enforcement-step3.md`
  - `scripts/ci/review-wave30-restrict-scope-enforcement-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave30 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves the bounded `restrict_scope` enforcement contract

## Restrict scope enforcement invariants
- [ ] Enforced deny reason remains present: `P_RESTRICT_SCOPE`
- [ ] Validation entrypoint remains present: `validate_restrict_scope`
- [ ] Failure reasons remain deterministic:
  - `scope_target_missing`
  - `scope_target_mismatch`
  - `scope_match_mode_unsupported`
  - `scope_type_unsupported`
- [ ] Scope evidence markers remain present:
  - `scope_type`
  - `scope_value`
  - `scope_match_mode`
  - `scope_evaluation_state`
  - `scope_failure_reason`
  - `restrict_scope_present`
  - `restrict_scope_target`
  - `restrict_scope_match`
  - `restrict_scope_reason`

## Non-goals still enforced
- [ ] No rewrite/filter behavior added
- [ ] No `redact_args` execution added
- [ ] No broad/global scope semantics added
- [ ] No control-plane/auth transport changes added

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
