# SPLIT REVIEW PACK - T-R1 Decision Emit Invariant Step2

## Summary
This Step2 converts `crates/assay-core/tests/decision_emit_invariant.rs` into one
directory-backed integration target rooted at `crates/assay-core/tests/decision_emit_invariant/main.rs`.

The split is mechanical:
- one Cargo integration target is preserved
- helpers move into `fixtures.rs`
- tests move by scenario family into dedicated submodules
- no production code or emitted-decision assertions are changed

## Review focus
- confirm `main.rs` is only module wiring
- confirm `fixtures.rs` contains shared helpers only
- confirm the moved tests still read as black-box contract tests
- confirm module-qualified selectors still run under `--test decision_emit_invariant`
- confirm no changes under `crates/assay-core/src/**`

## LOC shape
- `crates/assay-core/tests/decision_emit_invariant.rs`: `1293 -> deleted`
- `crates/assay-core/tests/decision_emit_invariant/main.rs`: `13`
- `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`: `130`
- `crates/assay-core/tests/decision_emit_invariant/emission.rs`: `407`
- `crates/assay-core/tests/decision_emit_invariant/approval.rs`: `113`
- `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`: `210`
- `crates/assay-core/tests/decision_emit_invariant/redaction.rs`: `222`
- `crates/assay-core/tests/decision_emit_invariant/guard.rs`: `46`
- `crates/assay-core/tests/decision_emit_invariant/delegation.rs`: `39`
- `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`: `110`
- new target total: `1290`

## Selector shape
- target name preserved: `cargo test -p assay-core --test decision_emit_invariant`
- exact test selectors are now module-qualified, e.g.:
  - `emission::test_policy_allow_emits_once`
  - `approval::approval_required_missing_denies`
  - `g3_auth::g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json`

## Validation
- `cargo fmt --all --check`
- `cargo clippy -q -p assay-core --all-targets -- -D warnings`
- `cargo test -q -p assay-core --test decision_emit_invariant`
- `BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step2.sh`
