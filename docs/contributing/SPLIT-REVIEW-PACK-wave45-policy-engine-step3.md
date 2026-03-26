# Wave45 Policy Engine Step3 Review Pack (Closure)

## Intent

Close the shipped Wave45 policy-engine split with docs/gates only and forbid post-Step2 redesign drift.

## Scope

- `docs/contributing/SPLIT-PLAN-wave45-policy-engine.md`
- `docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step3.md`
- `scripts/ci/review-wave45-policy-engine-step3.sh`

## Non-goals

- no workflow changes
- no changes under `crates/assay-core/src/mcp/policy/**`
- no changes under `crates/assay-core/tests/**`
- no new module cuts
- no policy redesign
- no allow/deny, precedence, or fail-closed drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave45-policy-engine-step3.sh
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

1. Confirm the diff is limited to the Step3 allowlist.
2. Confirm `crates/assay-core/src/mcp/policy/**` and `crates/assay-core/tests/**` are frozen in this wave.
3. Confirm the plan records `#961` as shipped and bounds Step3 to closure only.
4. Confirm the move-map freezes the current module ownership and does not propose another split.
5. Confirm the reviewer script re-runs the pinned policy invariants.
