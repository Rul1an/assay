# SPLIT-MOVE-MAP — T-R1 Step1 — `tests/decision_emit_invariant.rs`

## Goal

Freeze the split boundaries for `crates/assay-core/tests/decision_emit_invariant.rs` before any
mechanical module moves.

## Planned Step2 layout

- `crates/assay-core/tests/decision_emit_invariant/main.rs`
- `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`
- `crates/assay-core/tests/decision_emit_invariant/emission.rs`
- `crates/assay-core/tests/decision_emit_invariant/approval.rs`
- `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`
- `crates/assay-core/tests/decision_emit_invariant/redaction.rs`
- `crates/assay-core/tests/decision_emit_invariant/guard.rs`
- `crates/assay-core/tests/decision_emit_invariant/delegation.rs`
- `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`

## Mapping preview

- `main.rs` keeps the stable integration-target root, module wiring, and only the smallest shared prelude.
- `fixtures.rs` is the planned home for shared emitters, request builders, policy builders, and artifact builders.
- `emission.rs` is the planned home for cross-cutting emitted-decision contract tests:
  - allow/deny emission
  - multiple-call emission
  - required fields
  - event source
  - tool-call ID
  - non-tool-call error handling
- `approval.rs` is the planned home for:
  - `approval_required_*`
- `restrict_scope.rs` is the planned home for:
  - `restrict_scope_*`
- `redaction.rs` is the planned home for:
  - `redact_args_*`
- `guard.rs` is the planned home for guard lifecycle tests:
  - early return
  - panic emission
- `delegation.rs` is the planned home for additive delegation context tests.
- `g3_auth.rs` is the planned home for G3 auth projection and filtering tests.

## Frozen behavior boundaries

- identical integration-target identity for `decision_emit_invariant`
- identical emitted JSON contract coverage meaning
- identical delegation additive-field expectations
- identical approval/restrict-scope/redact-args deny semantics
- identical guard drop/panic emission expectations
- identical required-field and G3 auth projection expectations
- no edits under `crates/assay-core/src/**` in Step2 beyond the later explicitly planned production waves

## Test anchors to keep fixed in Step1

- `test_policy_allow_emits_once`
- `test_delegation_fields_are_additive_on_emitted_decisions`
- `approval_required_missing_denies`
- `restrict_scope_target_missing_denies`
- `redact_args_target_missing_denies`
- `test_guard_emits_on_panic`
- `test_event_contains_required_fields`
- `g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json`
