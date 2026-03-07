# Tool Call Handler Step2 Checklist (Mechanical)

Scope lock:
- `crates/assay-core/src/mcp/tool_call_handler.rs` (delete)
- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/types.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-tool-call-handler-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step2.md`
- `scripts/ci/review-tool-call-handler-step2.sh`
- optional module wiring: `crates/assay-core/src/mcp/mod.rs`

## Mechanical invariants

- `mod.rs` is facade-only (`new`, `with_lifecycle_emitter`, `handle_tool_call`).
- `evaluate.rs` contains policy/mandate routing logic.
- `emit.rs` is the only module constructing `DecisionEvent::new(...)`.
- `types.rs` contains type definitions and constructor wiring only.
- inline tests from old file moved to `tests.rs` with same test names.
- no workflow edits.

## Gate expectations

- allowlist-only diff vs `BASE_REF` (default `origin/main`)
- workflow-ban (`.github/workflows/*`)
- untracked-ban under `crates/assay-core/src/mcp/tool_call_handler/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `tool_taxonomy_policy_match_handler_decision_event_records_classes`
  - `test_event_contains_required_fields`

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-tool-call-handler-step2.sh` passes
- split remains behavior-identical (no API/path drift)
