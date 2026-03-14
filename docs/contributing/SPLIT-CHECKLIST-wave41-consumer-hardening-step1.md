# SPLIT CHECKLIST - Wave41 Consumer Hardening Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave41-consumer-hardening.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave41-consumer-hardening-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave41-consumer-hardening-step1.md`
  - `scripts/ci/review-wave41-consumer-hardening-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Consumer payload surfaces are explicit:
  - `DecisionEvent`
  - `DecisionData`
  - `ReplayDiffBasis`
- [ ] Deterministic consumer read precedence is explicit
- [ ] Required consumer signals are explicit
- [ ] Additive consumer compatibility expectations are explicit
- [ ] Non-goals are explicit (no runtime behavior change)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave41-consumer-hardening-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests pass
