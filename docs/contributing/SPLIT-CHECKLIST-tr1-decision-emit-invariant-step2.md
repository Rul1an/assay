# SPLIT CHECKLIST - T-R1 Decision Emit Invariant Step2

## Scope discipline
- [ ] Only these files changed:
  - `crates/assay-core/tests/decision_emit_invariant.rs`
  - `crates/assay-core/tests/decision_emit_invariant/main.rs`
  - `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`
  - `crates/assay-core/tests/decision_emit_invariant/emission.rs`
  - `crates/assay-core/tests/decision_emit_invariant/approval.rs`
  - `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`
  - `crates/assay-core/tests/decision_emit_invariant/redaction.rs`
  - `crates/assay-core/tests/decision_emit_invariant/guard.rs`
  - `crates/assay-core/tests/decision_emit_invariant/delegation.rs`
  - `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`
  - `docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md`
  - `docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step2.md`
  - `docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step2.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step2.md`
  - `scripts/ci/review-tr1-decision-emit-invariant-step2.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/src/**`
- [ ] No edits to other integration targets under `crates/assay-core/tests/**`

## Mechanical split contract
- [ ] The Cargo integration target identity remains `decision_emit_invariant`
- [ ] `tests/decision_emit_invariant/main.rs` is the stable target root
- [ ] `main.rs` contains module wiring only, not the bulk of test bodies
- [ ] `fixtures.rs` contains shared helpers only
- [ ] Scenario-family tests moved 1:1 into `emission.rs`, `approval.rs`, `restrict_scope.rs`, `redaction.rs`, `guard.rs`, `delegation.rs`, and `g3_auth.rs`
- [ ] The suite remains a black-box emitted-decision contract target
- [ ] No production behavior or emitted JSON assertions changed
- [ ] No second integration target was introduced
- [ ] Reviewer gates use module-qualified selectors that still run under `--test decision_emit_invariant`

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step2.sh` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy -q -p assay-core --all-targets -- -D warnings` passes
- [ ] `cargo test -q -p assay-core --test decision_emit_invariant` passes
