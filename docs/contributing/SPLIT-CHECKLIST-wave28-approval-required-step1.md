# SPLIT CHECKLIST — Wave28 Approval Required Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave28-approval-required.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave28-approval-required-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave28-approval-required-step1.md`
  - `scripts/ci/review-wave28-approval-required-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Approval validity checks are explicit
- [ ] Missing approval behavior is explicit
- [ ] Expired approval behavior is explicit
- [ ] Bound tool/resource mismatch behavior is explicit
- [ ] Evidence fields for approval enforcement are explicit
- [ ] Non-goals are explicit (no UI / external approval / restrict_scope / redact_args)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave28-approval-required-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
