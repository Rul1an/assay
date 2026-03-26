# Wave45 Policy Engine Step2 Review Pack (Mechanical Split)

## Intent

Perform the Wave45 mechanical split of `crates/assay-core/src/mcp/policy/engine.rs` into focused
helper modules while preserving policy behavior and downstream decision contracts.

## Scope

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

## Non-goals

- no workflow changes
- no changes under `crates/assay-core/tests/**`
- no policy-language redesign
- no precedence or fail-closed redesign
- no reason-code renames
- no handler / decision / evidence / CLI drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave45-policy-engine-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --test policy_engine_test test_mixed_tools_config -- --exact
cargo test -q -p assay-core --test policy_engine_test test_constraint_enforcement -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --lib 'mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only' -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to the Step2 allowlist.
2. Confirm `mod.rs` only adds `mod engine_next;` and keeps the public facade stable.
3. Confirm `engine.rs` still owns `evaluate_with_metadata(...)` and `check(...)` but no longer owns the extracted helpers.
4. Confirm no tests under `crates/assay-core/tests/**` changed.
5. Confirm allow/deny, precedence, and fail-closed invariants are pinned by the reviewer script.
