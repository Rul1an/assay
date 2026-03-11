# SPLIT CHECKLIST — Wave27 Approval Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave27-approval-artifact.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave27-approval-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave27-approval-step1.md`
  - `scripts/ci/review-wave27-approval-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Approval artifact minimum fields are explicit
- [ ] Freshness / expiry semantics are explicit
- [ ] Tool/resource binding is explicit
- [ ] Additive approval evidence fields are explicit
- [ ] Non-goals are explicit (`approval_required` enforcement not included yet)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave27-approval-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
