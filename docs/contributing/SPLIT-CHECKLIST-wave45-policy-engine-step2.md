# Wave45 Policy Engine Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/engine_next/mod.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`
- `docs/contributing/SPLIT-PLAN-wave45-policy-engine.md`
- `docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step2.md`
- `scripts/ci/review-wave45-policy-engine-step2.sh`
- no edits under `crates/assay-core/tests/**`
- no workflow edits

## Mechanical invariants

- `mod.rs` stays facade-only and adds only `mod engine_next;` wiring.
- `engine.rs` keeps `evaluate_with_metadata(...)` and `check(...)` as the stable routing facade.
- `engine.rs` no longer owns:
  - `check_rate_limits`
  - `finalize_evaluation`
  - `apply_approval_required_obligation`
  - `apply_restrict_scope_obligation`
  - `apply_redact_args_obligation`
  - `tool_drift_decision`
  - `schema_violation_decision`
  - `unconstrained_decision`
  - `is_denied`
  - `is_allowed`
  - `match_classes`
  - `classify_match_basis`
  - `matched_rule_name`
  - `parse_delegation_context`
- `engine_next/matcher.rs` owns tool/class matching helpers.
- `engine_next/effects.rs` owns obligation capture and contract evaluation helpers.
- `engine_next/precedence.rs` owns deny/allow precedence helpers.
- `engine_next/fail_closed.rs` owns tool-drift, rate-limit, schema-deny, and unconstrained fallback helpers.
- `engine_next/diagnostics.rs` owns metadata finalization and delegation parsing helpers.

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/tests/**`
- hard fail untracked files in `crates/assay-core/tests/**`
- hard fail untracked files under `crates/assay-core/src/mcp/policy/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `policy_engine_test::test_mixed_tools_config`
  - `policy_engine_test::test_constraint_enforcement`
  - `tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class`
  - `tool_taxonomy_policy_match_handler_decision_event_records_classes`
  - `approval_required_missing_denies`
  - `restrict_scope_target_missing_denies`
  - `redact_args_target_missing_denies`
  - `mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-wave45-policy-engine-step2.sh` passes
- split remains behavior-identical (no allow/deny/precedence/fail-closed drift)
- `engine.rs` is reduced to facade/routing logic and helper modules carry the extracted bodies
- no tests under `crates/assay-core/tests/**` changed
