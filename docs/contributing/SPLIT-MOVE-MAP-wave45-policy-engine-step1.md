# SPLIT-MOVE-MAP — Wave45 Step1 — `mcp/policy/engine.rs`

## Goal

Freeze the split boundaries for `crates/assay-core/src/mcp/policy/engine.rs` before any
mechanical module moves.

## Planned Step2 layout

- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`

## Mapping preview

- `engine.rs` keeps the stable routing surface for `evaluate_with_metadata(...)` and `check(...)`.
- `matcher.rs` is the planned home for tool/class matching helpers:
  - `is_denied`
  - `has_allowlist`
  - `is_allowed`
  - `matched_deny_classes`
  - `matched_allow_classes`
  - `match_classes`
  - `classify_match_basis`
  - `matched_rule_name`
- `effects.rs` is the planned home for obligation capture and contract evaluation helpers:
  - `apply_approval_required_obligation`
  - `apply_restrict_scope_obligation`
  - `apply_redact_args_obligation`
  - `default_restrict_scope_contract`
  - `default_redact_args_contract`
  - `evaluate_restrict_scope_contract`
  - `evaluate_redact_args_contract`
  - `redaction_target_value`
  - `can_apply_redaction`
- `precedence.rs` is the planned home for allow/deny precedence helpers and rule ordering glue
  extracted from `evaluate_with_metadata(...)`.
- `fail_closed.rs` is the planned home for default/fallback handling:
  - `check_rate_limits`
  - unconstrained-tool fallback handling
  - schema-missing / unsupported-path default behavior
- `diagnostics.rs` is the planned home for metadata and explainability helpers:
  - `finalize_evaluation`
  - `format_deny_contract`
  - `apply_delegation_context`
  - `parse_delegation_context`

## Frozen behavior boundaries

- identical allow/deny outcomes
- identical precedence behavior
- identical fail-closed behavior
- identical reason/policy codes
- no drift in downstream decision/event contracts
- no edits under `crates/assay-core/tests/**` in Step2

## Test anchors to keep fixed in Step1

- `policy_engine_test::test_mixed_tools_config`
- `policy_engine_test::test_constraint_enforcement`
- `tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class`
- `tool_taxonomy_policy_match_handler_decision_event_records_classes`
- `approval_required_missing_denies`
- `restrict_scope_target_missing_denies`
- `redact_args_target_missing_denies`
- `mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only`
