# Tool Call Handler Step1 Checklist (Freeze)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave16-tool-call-handler.md`
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step1.md`
- `scripts/ci/review-tool-call-handler-step1.sh`
- no code edits under `crates/assay-core/src/mcp/**`
- no workflow edits

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- hard fail tracked changes in `crates/assay-core/src/mcp/**`
- hard fail untracked files in `crates/assay-core/src/mcp/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `tool_taxonomy_policy_match_handler_decision_event_records_classes`
  - `test_event_contains_required_fields`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-tool-call-handler-step1.sh` passes
- Step1 diff contains only the 4 allowlisted files
