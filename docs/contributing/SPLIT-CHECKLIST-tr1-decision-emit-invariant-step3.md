# SPLIT CHECKLIST - T-R1 Decision Emit Invariant Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md`
  - `docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step3.md`
  - `docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step3.md`
  - `scripts/ci/review-tr1-decision-emit-invariant-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/tests/decision_emit_invariant/**`
- [ ] No edits under `crates/assay-core/src/**`

## Closure contract
- [ ] `tests/decision_emit_invariant/main.rs` remains the stable target root
- [ ] one integration target remains the final T-R1 shape
- [ ] no new module cuts are introduced
- [ ] no fixture reshuffle is introduced
- [ ] no emitted-contract selector or assertion semantics are changed in Step3

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step3.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -q -p assay-core --all-targets -- -D warnings` passes
