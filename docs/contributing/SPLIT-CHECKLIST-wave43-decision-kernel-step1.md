# SPLIT CHECKLIST - Wave43 Decision Kernel Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step1.md`
  - `scripts/ci/review-wave43-decision-kernel-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/src/mcp/**`
- [ ] No edits under `crates/assay-core/tests/**`
- [ ] No CLI or MCP server changes

## Contract freeze
- [ ] Stable `decision.rs` facade is explicit
- [ ] Stable public decision/event symbols are explicit
- [ ] Proposed `decision_next/` boundaries are explicit
- [ ] Non-goals are explicit
- [ ] Event payload shape freeze is explicit
- [ ] Reason-code freeze is explicit
- [ ] Replay/contract refresh freeze is explicit

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave43-decision-kernel-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core --all-targets -- -D warnings` passes
- [ ] Pinned decision/replay tests pass
