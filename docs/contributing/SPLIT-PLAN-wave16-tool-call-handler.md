# Wave16 Plan — `mcp/tool_call_handler.rs` Split

## Goal

Split `crates/assay-core/src/mcp/tool_call_handler.rs` into bounded modules with zero behavior change and stable public API.

## Step1 (freeze)

Branch: `codex/wave16-tool-call-handler-step1-freeze` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave16-tool-call-handler.md`
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step1.md`
- `scripts/ci/review-tool-call-handler-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-core/src/mcp/**`
- no workflow edits

Step1 gate:
- allowlist-only diff (the 4 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-core/src/mcp/**`
- hard fail on untracked files in `crates/assay-core/src/mcp/**`
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -p assay-core test_event_contains_required_fields -- --exact`

## Step2 (mechanical split preview)

Target layout (preview):
- `crates/assay-core/src/mcp/tool_call_handler/mod.rs` (facade + public API)
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/types.rs`
- optional tests module relocation if inline tests exist

Step2 principles:
- 1:1 body moves
- stable public surface (`ToolCallHandler`, `ToolCallHandlerConfig`, `HandleResult`)
- no behavior changes in decision events / required field emission

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and keeps allowlist strict.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once chain is clean.
