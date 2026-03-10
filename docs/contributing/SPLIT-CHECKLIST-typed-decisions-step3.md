# SPLIT CHECKLIST — Wave24 Typed Decisions Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step3.md`
  - `scripts/ci/review-wave24-typed-decisions-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave24 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves the frozen compatibility rule for `AllowWithWarning`

## Typed decision invariants
- [ ] Typed decision markers remain present:
  - `allow_with_obligations`
  - `deny_with_alert`
- [ ] `AllowWithWarning` compatibility path remains present
- [ ] Decision Event v2 field markers remain present:
  - `policy_version`
  - `policy_digest`
  - `obligations`
  - `approval_state`
  - `lane`
  - `principal`
  - `auth_context_summary`
- [ ] Existing event fields remain present:
  - `tool_classes`
  - `matched_tool_classes`
  - `match_basis`
  - `matched_rule`
  - `reason_code`

## Non-goals still enforced
- [ ] No obligations execution added
- [ ] No approval enforcement added
- [ ] No new policy backend introduced
- [ ] No auth transport model changes introduced

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
