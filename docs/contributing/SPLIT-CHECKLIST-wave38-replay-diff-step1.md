# SPLIT CHECKLIST - Wave38 Replay Diff Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave38-replay-diff-contract.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave38-replay-diff-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave38-replay-diff-step1.md`
  - `scripts/ci/review-wave38-replay-diff-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Replay basis fields are explicit
- [ ] Diff buckets are explicit
- [ ] Deterministic semantics are explicit
- [ ] Additive contract intent is explicit
- [ ] Non-goals are explicit (no runtime capability expansion)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave38-replay-diff-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
