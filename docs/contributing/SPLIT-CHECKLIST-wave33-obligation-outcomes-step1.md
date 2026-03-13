# SPLIT CHECKLIST - Wave33 Obligation Outcomes Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave33-obligation-outcomes-normalization.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave33-obligation-outcomes-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave33-obligation-outcomes-step1.md`
  - `scripts/ci/review-wave33-obligation-outcomes-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Normalized `obligation_outcomes` fields are explicit
- [ ] Canonical status semantics (`applied|skipped|error`) are explicit
- [ ] Reason-code baseline is explicit
- [ ] Additive compatibility rule is explicit
- [ ] Non-goals are explicit (no runtime behavior changes)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave33-obligation-outcomes-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
