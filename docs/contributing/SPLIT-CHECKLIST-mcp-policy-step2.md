# MCP Policy Step2 Checklist (Mechanical Split)

Scope lock:
- `crates/assay-core/src/mcp/policy.rs` (deleted as part of split)
- `crates/assay-core/src/mcp/policy/**`
- Step2 docs + reviewer script only
- no workflow changes

## Required outputs

- `docs/contributing/SPLIT-CHECKLIST-mcp-policy-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mcp-policy-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step2.md`
- `scripts/ci/review-mcp-policy-step2.sh`

## Mechanical invariants

- `mcp/policy/mod.rs` is facade-first: public surface + wrappers only.
- `evaluate_with_metadata` delegates to `engine::evaluate_with_metadata(...)`.
- `check` delegates to `engine::check(...)`.
- schema compilation and legacy normalization/deprecation handling moved out of facade.
- `make_deny_response` public symbol is preserved at `mcp::policy::make_deny_response`.
- no workflow edits.

## Must-survive tests

- `tool_taxonomy_policy_match_handler_decision_event_records_classes`
- `test_event_contains_required_fields`
- `test_mixed_tools_config`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests above (`-- --exact`)
- allowlist-only diff
- workflow-ban
- facade wrapper invariants

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step2 diff stays inside Step2 allowlist
- targeted policy-path tests remain green
