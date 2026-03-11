# SPLIT CHECKLIST - Wave27 Approval Step2

## Scope discipline
- [ ] Step2 contains bounded approval artifact/data-shape implementation only
- [ ] No `.github/workflows/*` changes
- [ ] No runtime enforcement of `approval_required`
- [ ] No approval UI/case-management changes
- [ ] No external approval services integration
- [ ] No control-plane or auth transport changes

## Approval artifact contract checks
- [ ] `approval_id` is represented in runtime policy/event path
- [ ] `approver` is represented in runtime policy/event path
- [ ] `issued_at` is represented in runtime policy/event path
- [ ] `expires_at` is represented in runtime policy/event path
- [ ] `scope` is represented in runtime policy/event path
- [ ] `bound_tool` is represented in runtime policy/event path
- [ ] `bound_resource` is represented in runtime policy/event path
- [ ] `approval_freshness` is represented in runtime policy/event path

## Evidence and compatibility checks
- [ ] Approval evidence fields are additive and backward-compatible
- [ ] Existing `approval_state` remains available
- [ ] Existing typed decision and obligations `log`/`alert` behavior remains stable
- [ ] Existing event consumers remain compatible

## Non-goals enforced
- [ ] No runtime blocking on missing approval
- [ ] No runtime blocking on expired approval
- [ ] No `restrict_scope` execution
- [ ] No `redact_args` execution

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned core/cli/server tests remain green
