# SPLIT-MOVE-MAP — Wave45 Step2 — `mcp/policy/engine.rs`

## Goal

Mechanically split `crates/assay-core/src/mcp/policy/engine.rs` into focused helper modules with
zero policy semantic drift and stable policy entrypoints.

## New layout

- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/engine_next/mod.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`

## Mapping table

- Top-level evaluation flow, `evaluate_with_metadata(...)`, and `check(...)` stay in `engine.rs`.
- Matching helpers move to `engine_next/matcher.rs`:
  - `is_denied`
  - `has_allowlist`
  - `is_allowed`
  - `matched_deny_classes`
  - `matched_allow_classes`
  - `match_classes`
  - `classify_match_basis`
  - `matched_rule_name`
- Obligation and contract-evaluation helpers move to `engine_next/effects.rs`:
  - `apply_approval_required_obligation`
  - `apply_restrict_scope_obligation`
  - `apply_redact_args_obligation`
  - `default_restrict_scope_contract`
  - `default_redact_args_contract`
  - `evaluate_restrict_scope_contract`
  - `evaluate_redact_args_contract`
  - `redaction_target_value`
  - `can_apply_redaction`
- Precedence helpers move to `engine_next/precedence.rs`:
  - deny-list matching branch
  - allow-list gate branch
  - allow metadata projection
- Fail-closed/default helpers move to `engine_next/fail_closed.rs`:
  - `tool_drift_decision`
  - `check_rate_limits`
  - `schema_violation_decision`
  - `unconstrained_decision`
- Metadata and diagnostics helpers move to `engine_next/diagnostics.rs`:
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
- `crates/assay-core/tests/**` remain untouched in Step2

## Post-split shape

- `engine.rs`: `799 -> facade target <= 320 LOC`
- helper logic is split into `engine_next/*` behind the same policy entrypoints
- no new public API surface added
