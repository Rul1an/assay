# SPLIT CHECKLIST - Wave43 Decision Kernel Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step3.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step3.md`
  - `scripts/ci/review-wave43-decision-kernel-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/src/mcp/**`
- [ ] No edits under `crates/assay-core/tests/**`
- [ ] No CLI or MCP server changes

## Step3 closure contract
- [ ] Step2 is recorded as shipped behind a stable facade
- [ ] Step3 is explicitly bounded to micro-cleanup only
- [ ] No new module cuts are proposed in Step3
- [ ] No payload / reason-code / replay drift is allowed in Step3
- [ ] `decision.rs` remains the stable public entry point
- [ ] `decision_next/*` remains the split implementation ownership boundary

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave43-decision-kernel-step3.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core --all-targets -- -D warnings` passes
- [ ] Pinned decision/replay invariants pass
