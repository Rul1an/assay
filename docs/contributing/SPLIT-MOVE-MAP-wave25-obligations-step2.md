# SPLIT-MOVE-MAP - Wave25 Step2 - Obligations Log Execution

## Goal
Execute the first bounded obligations runtime slice without widening policy scope:
1. execute `log` obligations
2. map `legacy_warning` to `log` execution
3. emit additive `obligation_outcomes` evidence

## File-level map

### Core runtime
- `crates/assay-core/src/mcp/obligations.rs`
  - Wave25 execution function: `execute_log_only`
  - `log` -> `applied`
  - `legacy_warning` -> `log` + `applied`
  - unknown -> `skipped`

- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - execute obligations in the runtime decision path
  - propagate outcomes into policy context

- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - include obligation outcomes in event context plumbing

- `crates/assay-core/src/mcp/proxy.rs`
  - keep proxy decision event parity with runtime outcomes

- `crates/assay-core/src/mcp/decision.rs`
  - additive event schema field: `obligation_outcomes`
  - typed status model: `applied | skipped | error`

- `crates/assay-core/src/mcp/mod.rs`
  - expose obligations module

### Tests
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - verify `legacy_warning` compat emits `log` outcome

- `crates/assay-core/src/mcp/obligations.rs` (unit tests)
  - verify `log`, `legacy_warning`, and unknown type behavior

- `crates/assay-core/tests/decision_emit_invariant.rs`
  - verify `obligation_outcomes` in decision event invariant path

## Behavior parity notes
- Allow/deny final decisions remain unchanged.
- Mandate flow remains unchanged.
- Obligation execution is informational/non-blocking in this wave.

## Out-of-scope guardrails
- no `approval_required` enforcement
- no `restrict_scope` enforcement
- no `redact_args` enforcement
- no fail-closed redesign in this wave
