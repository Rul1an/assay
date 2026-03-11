# SPLIT CHECKLIST — Wave28 Approval Required Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave28-approval-required-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave28-approval-required-step3.md`
  - `scripts/ci/review-wave28-approval-required-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave28 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves the bounded `approval_required` enforcement contract

## Approval enforcement invariants
- [ ] Approval artifact/evidence markers remain present:
  - `approval_id`
  - `approver`
  - `issued_at`
  - `expires_at`
  - `scope`
  - `approval_bound_tool`
  - `approval_bound_resource`
  - `approval_freshness`
  - `approval_state`
- [ ] `approval_required` runtime marker remains present
- [ ] Missing approval deny behavior remains present
- [ ] Expired approval deny behavior remains present
- [ ] Bound tool mismatch deny behavior remains present
- [ ] Bound resource mismatch deny behavior remains present
- [ ] `approval_failure_reason` remains present

## Non-goals still enforced
- [ ] No approval UI added
- [ ] No case-management added
- [ ] No external approval service added
- [ ] No `restrict_scope` execution added
- [ ] No `redact_args` execution added
- [ ] No grace/renewal/global approval semantics added

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `legacy_warning -> log` compatibility remains present
- [ ] `obligation_outcomes` remains additive

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
