# SPLIT CHECKLIST - Wave27 Approval Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave27-approval-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step3.md`
  - `scripts/ci/review-wave27-approval-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave27 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope

## Step2 invariants preserved
- [ ] Approval artifact/data shape markers remain present:
  - `approval_id`
  - `approver`
  - `issued_at`
  - `expires_at`
  - `scope`
  - `bound_tool`
  - `bound_resource`
- [ ] Approval evidence markers remain present:
  - `approval_state`
  - `approval_freshness`
- [ ] Existing typed decision + obligations behavior remains in place
- [ ] No approval enforcement markers are introduced

## Non-goals still enforced
- [ ] No runtime enforcement of `approval_required`
- [ ] No approval UI/case-management
- [ ] No external approval services
- [ ] No control-plane/auth transport changes

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned core/cli/server tests remain green
