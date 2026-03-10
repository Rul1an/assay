# SPLIT REVIEW PACK — Wave24 Typed Decisions Step1

## Intent
Freeze the runtime decision contract and Decision Event v2 shape before implementation.

This slice is docs + gate only.

It must not:
- change MCP runtime code
- change CLI normalization code
- change MCP server behavior
- change workflow files

## Allowed files
- `docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md`
- `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step1.md`
- `scripts/ci/review-wave24-typed-decisions-step1.sh`

## What reviewers should verify
1. Diff is limited to the four Step1 files.
2. The typed decision target model is explicit.
3. `AllowWithWarning` compatibility is explicit.
4. Decision Event v2 fields are explicit.
5. The wave is contract-only and does not sneak in execution semantics.
6. Existing MCP/runtime paths are untouched.

## Reviewer command
```bash
BASE_REF=origin/main bash scripts/ci/review-wave24-typed-decisions-step1.sh
```

Expected outcome:
- gate passes
- runtime code is untouched
- contract scope is frozen cleanly
- Step2 can implement without reopening semantics

Gate includes:
```bash
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration
```
