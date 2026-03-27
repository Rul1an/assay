# T-R1 Decision Emit Invariant Step1 Review Pack

## Intent

Freeze the split boundaries for `crates/assay-core/tests/decision_emit_invariant.rs` before any
mechanical module moves, while preserving one coherent black-box integration-test contract target.

## Scope

- `docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md`
- `docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step1.md`
- `scripts/ci/review-tr1-decision-emit-invariant-step1.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/**`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no production behavior changes
- no test-target fragmentation or helper reorganization beyond the Step2 preview

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step1.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --test decision_emit_invariant test_policy_allow_emits_once -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_guard_emits_on_panic -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact
cargo test -q -p assay-core --test decision_emit_invariant g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the 5 Step1 files.
2. Confirm the plan keeps `decision_emit_invariant` as one integration target.
3. Confirm `main.rs` is planned as a thin root rather than a second giant test file.
4. Confirm `fixtures.rs` is constrained to shared helpers only.
5. Confirm the reviewer script re-runs delegation, deny-path, guard, required-field, and G3 auth anchors.
