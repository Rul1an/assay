# SPLIT MOVE MAP - T-R1 Decision Emit Invariant Step3

## Intent
Record that T-R1 is closed with the Step2 target shape preserved unchanged.

## Final target ownership
- `crates/assay-core/tests/decision_emit_invariant/main.rs`
  - stable integration target root
  - module wiring only
- `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`
  - shared black-box helpers only
- `crates/assay-core/tests/decision_emit_invariant/emission.rs`
  - allow/deny/emission contract tests
- `crates/assay-core/tests/decision_emit_invariant/approval.rs`
  - approval-required contract tests
- `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`
  - restrict-scope contract tests
- `crates/assay-core/tests/decision_emit_invariant/redaction.rs`
  - redact-args contract tests
- `crates/assay-core/tests/decision_emit_invariant/guard.rs`
  - guard fallback contract tests
- `crates/assay-core/tests/decision_emit_invariant/delegation.rs`
  - delegation additive-field contract tests
- `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`
  - G3 auth projection contract tests

## Explicit closure claim
- no additional movement belongs in Step3
- T-R1 is complete once this closure slice lands
- any future change to this target is a new wave, not part of T-R1
