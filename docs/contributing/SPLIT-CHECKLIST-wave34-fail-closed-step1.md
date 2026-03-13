# SPLIT CHECKLIST - Wave34 Fail-Closed Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave34-fail-closed-matrix-typing.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave34-fail-closed-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave34-fail-closed-step1.md`
  - `scripts/ci/review-wave34-fail-closed-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Fail-closed matrix dimensions are explicit
- [ ] Risk class values are explicit
- [ ] Fallback mode values are explicit
- [ ] Trigger baseline is explicit
- [ ] Fail-closed reason-code baseline is explicit
- [ ] Additive compatibility rule is explicit
- [ ] Non-goals are explicit

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave34-fail-closed-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
