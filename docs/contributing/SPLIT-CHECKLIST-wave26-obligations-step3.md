# SPLIT CHECKLIST - Wave26 Obligations Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave26-obligations-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave26-obligations-step3.md`
  - `scripts/ci/review-wave26-obligations-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server behavior changes

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 remains green against stacked base and `origin/main`

## Step2 invariants retained
- [ ] Executable obligations scope stays bounded to:
  - `log`
  - `alert`
- [ ] `legacy_warning` -> `log` compatibility remains intact
- [ ] `obligation_outcomes` stays additive
- [ ] No high-risk obligations execution added:
  - `approval_required`
  - `restrict_scope`
  - `redact_args`
- [ ] No external incident/case-management integration markers added

## Validation
- [ ] `BASE_REF=origin/codex/wave26-obligations-alert-step2-impl bash scripts/ci/review-wave26-obligations-step3.sh` passes
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave26-obligations-step3.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests remain green
