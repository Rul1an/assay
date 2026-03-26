# SPLIT CHECKLIST - Wave43 Decision Kernel Step2

## Scope discipline
- [ ] Only these files changed:
  - `crates/assay-core/src/mcp/decision.rs`
  - `crates/assay-core/src/mcp/decision_next/mod.rs`
  - `crates/assay-core/src/mcp/decision_next/event_types.rs`
  - `crates/assay-core/src/mcp/decision_next/normalization.rs`
  - `crates/assay-core/src/mcp/decision_next/builder.rs`
  - `crates/assay-core/src/mcp/decision_next/emitters.rs`
  - `crates/assay-core/src/mcp/decision_next/guard.rs`
  - `docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step2.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step2.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step2.md`
  - `scripts/ci/review-wave43-decision-kernel-step2.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/tests/**`
- [ ] No edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- [ ] No edits under `crates/assay-core/src/mcp/policy/**`
- [ ] No CLI or MCP server changes

## Mechanical split contract
- [ ] `decision.rs` remains the stable facade and public entry point
- [ ] `decision_next/` contains only the moved implementation blocks
- [ ] `Decision`, `DecisionEvent`, `DecisionData`, `DecisionEmitter`, `DecisionEmitterGuard`, and `reason_codes` remain compatible
- [ ] No event payload shape changes are introduced
- [ ] No reason-code renames are introduced
- [ ] No replay/contract refresh behavior changes are introduced
- [ ] Inline unit tests remain in `decision.rs`

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave43-decision-kernel-step2.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -p assay-core --all-targets -- -D warnings` passes
- [ ] Pinned decision/replay invariants pass
