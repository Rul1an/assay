# MCP Policy Step1 Review Pack (Freeze)

## Intent

Freeze Wave15 scope for `mcp/policy.rs` split before any code movement.

## Scope

- `docs/contributing/SPLIT-PLAN-wave15-mcp-policy.md`
- `docs/contributing/SPLIT-CHECKLIST-mcp-policy-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step1.md`
- `scripts/ci/review-mcp-policy-step1.sh`

## Non-goals

- no edits in `crates/assay-core/src/mcp/**`
- no behavior changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-mcp-policy-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core test_mixed_tools_config -- --exact
```

## Reviewer 60s scan

1. Confirm only Step1 docs/script changed.
2. Confirm no `crates/assay-core/src/mcp/**` tracked/untracked changes.
3. Confirm Step2 and Step4 process is explicit.
4. Run reviewer script and expect PASS.
