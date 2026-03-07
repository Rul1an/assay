# MCP Policy Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-mcp-policy-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step3.md`
- `scripts/ci/review-mcp-policy-step3.sh`
- no code changes in Step3
- no workflow changes

## Closure invariants

- re-run Step2 quality checks (`fmt`, `clippy`, targeted tests)
- re-run Step2 facade invariants (`mod.rs` wrappers + thinness)
- re-run Step2 visibility invariants (`engine/schema/legacy` expose only `pub(super)`)
- keep `make_deny_response` re-export invariant in facade

## Gate requirements

- allowlist-only diff vs Step2 base branch
- workflow-ban (`.github/workflows/*`)
- quality checks:
  - `cargo fmt --check`
  - `cargo clippy -p assay-core --all-targets -- -D warnings`
  - `cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -p assay-core test_event_contains_required_fields -- --exact`
  - `cargo test -p assay-core test_mixed_tools_config -- --exact`

## Definition of done

- `BASE_REF=origin/codex/wave15-mcp-policy-step2-mechanical bash scripts/ci/review-mcp-policy-step3.sh` passes
- Step3 diff is docs+script only
