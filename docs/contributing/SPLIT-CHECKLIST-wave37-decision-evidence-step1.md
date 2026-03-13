# SPLIT CHECKLIST — Wave37 Decision Evidence Convergence Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave37-decision-evidence-convergence.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave37-decision-evidence-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave37-decision-evidence-step1.md`
  - `scripts/ci/review-wave37-decision-evidence-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] canonical outcome taxonomy is explicit
- [ ] deterministic classification semantics are explicit
- [ ] additive convergence evidence fields are explicit
- [ ] downstream compatibility rules are explicit
- [ ] non-goals are explicit (no new runtime capability in Wave37)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave37-decision-evidence-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
