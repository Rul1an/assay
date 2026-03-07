# Tool Call Handler Step2 Review Pack (Mechanical Split)

## Intent

Perform Wave16 mechanical split of `crates/assay-core/src/mcp/tool_call_handler.rs` into focused modules while preserving behavior and public API.

## Scope

- `crates/assay-core/src/mcp/tool_call_handler.rs` (deleted)
- `crates/assay-core/src/mcp/tool_call_handler/mod.rs`
- `crates/assay-core/src/mcp/tool_call_handler/types.rs`
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-tool-call-handler-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step2.md`
- `scripts/ci/review-tool-call-handler-step2.sh`

## Non-goals

- no workflow changes
- no MCP policy contract redesign
- no decision-event schema changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tool-call-handler-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
```

## Reviewer 60s scan

1. Confirm diff is limited to Step2 allowlist.
2. Confirm `mod.rs` is thin facade wrappers only.
3. Confirm `DecisionEvent::new(...)` only appears in `emit.rs`.
4. Confirm moved unit tests exist in `tests.rs` with same names.
5. Confirm targeted contract tests remain green.
