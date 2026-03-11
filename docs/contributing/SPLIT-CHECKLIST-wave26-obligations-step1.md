# SPLIT CHECKLIST - Wave26 Obligations Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave26-obligations-alert.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step1.md`
  - `scripts/ci/review-wave26-obligations-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Wave26 execution scope is frozen to `log` + `alert`
- [ ] `legacy_warning` compatibility path is explicitly preserved
- [ ] `obligation_outcomes` remains additive and unchanged at schema level
- [ ] Non-goals are explicit (`approval_required`, `restrict_scope`, `redact_args`)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave26-obligations-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
