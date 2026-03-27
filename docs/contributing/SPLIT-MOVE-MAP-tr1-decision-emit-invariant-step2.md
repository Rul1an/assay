# SPLIT MOVE MAP - T-R1 Decision Emit Invariant Step2

## Intent
Record the exact movement from the single-file integration target
`crates/assay-core/tests/decision_emit_invariant.rs` into one
directory-backed integration target rooted at
`crates/assay-core/tests/decision_emit_invariant/main.rs`.

## File ownership after Step2
- `crates/assay-core/tests/decision_emit_invariant/main.rs`
  - target root
  - module wiring only
- `crates/assay-core/tests/decision_emit_invariant/fixtures.rs`
  - `TestEmitter`
  - `make_tool_request`
  - `make_tool_request_with_args`
  - `approval_required_policy`
  - `restrict_scope_policy_with_contract`
  - `restrict_scope_policy`
  - `redact_args_policy_with_contract`
  - `redact_args_policy`
  - `approval_artifact`
- `crates/assay-core/tests/decision_emit_invariant/emission.rs`
  - `test_policy_allow_emits_once`
  - `test_policy_deny_emits_once`
  - `test_commit_tool_no_mandate_emits_deny`
  - `test_alert_obligation_outcome_emitted`
  - `test_multiple_calls_emit_multiple_events`
  - `test_event_source_from_config`
  - `test_tool_call_id_propagated`
  - `test_non_tool_call_emits_error`
  - `test_event_contains_required_fields`
- `crates/assay-core/tests/decision_emit_invariant/approval.rs`
  - `approval_required_missing_denies`
  - `approval_required_expired_denies`
  - `approval_required_bound_tool_mismatch_denies`
  - `approval_required_bound_resource_mismatch_denies`
- `crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs`
  - `restrict_scope_mismatch_denies`
  - `restrict_scope_mismatch_does_not_deny`
  - `restrict_scope_match_sets_additive_fields`
  - `restrict_scope_target_missing_denies`
  - `restrict_scope_unsupported_match_mode_denies`
  - `restrict_scope_unsupported_scope_type_denies`
- `crates/assay-core/tests/decision_emit_invariant/redaction.rs`
  - `redact_args_contract_sets_additive_fields`
  - `redact_args_target_missing_denies`
  - `redact_args_mode_unsupported_denies`
  - `redact_args_scope_unsupported_denies`
  - `redact_args_apply_failed_denies`
- `crates/assay-core/tests/decision_emit_invariant/guard.rs`
  - `test_guard_drop_emits_on_early_return`
  - `test_guard_emits_on_panic`
- `crates/assay-core/tests/decision_emit_invariant/delegation.rs`
  - `test_delegation_fields_are_additive_on_emitted_decisions`
- `crates/assay-core/tests/decision_emit_invariant/g3_auth.rs`
  - `SYNTHETIC_JWS_COMPACT`
  - `g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json`
  - `g3_unknown_auth_scheme_dropped_whitespace_principal_absent_in_decision_json`
  - `g3_jwt_and_bearer_material_never_appear_on_emitted_decision_json`

## Selector change note
- The integration target name stays `decision_emit_invariant`.
- Exact test selectors are now module-qualified, for example:
  - `emission::test_policy_allow_emits_once`
  - `delegation::test_delegation_fields_are_additive_on_emitted_decisions`
  - `approval::approval_required_missing_denies`
  - `g3_auth::g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json`

## Explicit non-moves
- `crates/assay-core/src/mcp/**`
  - unchanged
- other integration targets under `crates/assay-core/tests/**`
  - unchanged
- target identity
  - still one `cargo test -p assay-core --test decision_emit_invariant` binary
