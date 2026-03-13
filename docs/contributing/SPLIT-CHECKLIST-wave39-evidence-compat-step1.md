# SPLIT CHECKLIST - Wave39 Evidence Compat Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave39-evidence-compat-normalization.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave39-evidence-compat-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave39-evidence-compat-step1.md`
  - `scripts/ci/review-wave39-evidence-compat-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Replay/evidence compatibility contract is explicit
- [ ] Additive legacy fallback semantics are explicit
- [ ] Deterministic classification precedence is explicit
- [ ] Required compatibility markers are explicit:
  - `decision_basis_version`
  - `compat_fallback_applied`
  - `classification_source`
  - `replay_diff_reason`
  - `legacy_shape_detected`
- [ ] Non-goals are explicit (no runtime capability expansion)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave39-evidence-compat-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests pass
