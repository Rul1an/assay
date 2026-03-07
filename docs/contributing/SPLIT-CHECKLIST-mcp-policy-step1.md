# MCP Policy Step1 Checklist (Freeze)

Scope lock:
- docs + reviewer gate script only
- no workflow changes
- no edits under `crates/assay-core/src/mcp/**`

## Required outputs

- `docs/contributing/SPLIT-PLAN-wave15-mcp-policy.md`
- `docs/contributing/SPLIT-CHECKLIST-mcp-policy-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step1.md`
- `scripts/ci/review-mcp-policy-step1.sh`

## Freeze requirements

- Step2 module target layout documented
- Step4 promote flow documented
- no tracked changes in `crates/assay-core/src/mcp/**`
- no untracked files in `crates/assay-core/src/mcp/**`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
- `cargo test -p assay-core test_event_contains_required_fields -- --exact`
- `cargo test -p assay-core test_mixed_tools_config -- --exact`
- allowlist-only diff
- workflow-ban

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step1 diff is limited to the four freeze files
