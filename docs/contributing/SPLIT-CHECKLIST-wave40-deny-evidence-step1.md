# SPLIT CHECKLIST - Wave40 Deny Evidence Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave40-deny-evidence-convergence.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave40-deny-evidence-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave40-deny-evidence-step1.md`
  - `scripts/ci/review-wave40-deny-evidence-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Deny separation markers are explicit:
  - `policy_deny`
  - `fail_closed_deny`
  - `enforcement_deny`
- [ ] Deterministic deny precedence is explicit
- [ ] Legacy deny fallback compatibility is explicit
- [ ] Non-goals are explicit (no runtime behavior change)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave40-deny-evidence-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests pass
