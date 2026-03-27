# SPLIT CHECKLIST - T-R2 Tool Call Handler Tests Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md`
  - `docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step3.md`
  - `docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step3.md`
  - `scripts/ci/review-tr2-tool-call-handler-tests-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- [ ] No edits under `crates/assay-core/tests/**`
- [ ] No edits under `crates/assay-core/src/mcp/policy/**`
- [ ] No edits to `crates/assay-core/src/mcp/decision.rs`

## Closure contract
- [ ] `crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs` remains the stable unit-test root
- [ ] the Step2 module tree remains the final T-R2 shape
- [ ] no new module cuts are introduced
- [ ] no fixture reshuffle is introduced
- [ ] no private-access or selector semantics are changed in Step3

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step3.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -q -p assay-core --all-targets -- -D warnings` passes
