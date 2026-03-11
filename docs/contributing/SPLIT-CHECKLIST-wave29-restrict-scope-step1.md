# SPLIT CHECKLIST — Wave29 Restrict Scope Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave29-restrict-scope.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave29-restrict-scope-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave29-restrict-scope-step1.md`
  - `scripts/ci/review-wave29-restrict-scope-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] `restrict_scope` contract fields are explicit
- [ ] Scope match/mismatch semantics are explicit
- [ ] Scope mismatch reasons are explicit
- [ ] Additive scope evidence fields are explicit
- [ ] Non-goals are explicit (`restrict_scope` runtime enforcement not included yet)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave29-restrict-scope-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
