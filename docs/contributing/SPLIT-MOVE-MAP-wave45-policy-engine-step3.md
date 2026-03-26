# SPLIT-MOVE-MAP — Wave45 Step3 — `mcp/policy/engine.rs` Closure

## Shipped layout

Wave45 Step2 is now the shipped split shape on `main`:
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/engine_next/mod.rs`
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`

## Ownership freeze

- `crates/assay-core/src/mcp/policy/engine.rs`
  remains the stable facade for policy entrypoints and top-level routing.
- `crates/assay-core/src/mcp/policy/engine_next/matcher.rs`
  remains the matching helper boundary.
- `crates/assay-core/src/mcp/policy/engine_next/effects.rs`
  remains the obligation and contract-evaluation boundary.
- `crates/assay-core/src/mcp/policy/engine_next/precedence.rs`
  remains the ordering and allow/deny precedence boundary.
- `crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs`
  remains the fail-closed and fallback boundary.
- `crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs`
  remains the metadata finalization and delegation parsing boundary.

## Allowed follow-up after closure

- documentation updates only
- reviewer-gate tightening only
- internal visibility tightening only if it requires no code edits in this wave

## Explicitly deferred

- new module cuts
- matcher or DSL redesign
- allow/deny or fail-closed behavior cleanup
- reason-code or precedence changes
- handler, decision, evidence, or CLI coupling changes
