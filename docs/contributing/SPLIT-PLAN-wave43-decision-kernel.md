# SPLIT PLAN - Wave43 Decision Kernel Split

## Intent
Freeze a bounded split plan for `crates/assay-core/src/mcp/decision.rs` before any mechanical
module moves.

This wave is about:
- turning the current large decision kernel into a thin facade behind internal modules
- keeping the public decision/event contract stable
- isolating emitter, guard, normalization, and replay-facing logic into reviewable seams
- locking reviewer gates before any Step2 code movement

It explicitly does **not** add:
- new MCP runtime behavior
- new decision/event fields
- policy engine behavior changes
- tool-call handler behavior changes
- CLI or MCP server behavior changes
- workflow changes

## Problem
`crates/assay-core/src/mcp/decision.rs` is currently one of the largest handwritten production
Rust files on `main` (baseline inventory: 1426 LOC at `origin/main` `47b67769`).

The file is already partially decomposed through internal submodules:
- `consumer_contract`
- `context_contract`
- `deny_convergence`
- `outcome_convergence`
- `replay_compat`
- `replay_diff`

But the main facade still co-locates too much:
- core event/data types
- builder-style event construction
- emitter implementations
- guard lifecycle
- fulfillment normalization
- contract projection refresh
- inline unit tests

That makes review and later bounded changes harder than they need to be.

## Frozen public surface
Wave43 freezes the expectation that Step2 keeps these public or externally relied-on surfaces
stable in meaning:
- `Decision`
- `DecisionEvent`
- `DecisionData`
- `ObligationOutcome`
- `PolicyDecisionEventContext`
- `DecisionEmitter`
- `DecisionEmitterGuard`
- `FileDecisionEmitter`
- `NullDecisionEmitter`
- `refresh_contract_projections`
- `reason_codes`

Step2 may reorganize implementation ownership behind these surfaces, but must not redefine their
contract.

## Frozen contract rules
Wave43 freezes the following rules for the split:

1. `decision.rs` becomes thinner, but remains the stable facade.
2. No event payload shape changes are allowed in this wave.
3. No reason-code renames are allowed in this wave.
4. No replay-basis behavior changes are allowed in this wave.
5. Existing internal convergence modules remain semantically stable while the main file is split.
6. Step2 is mechanical-first: move bodies 1:1 before any cleanup.

## Suggested Step2 target layout

```text
crates/assay-core/src/mcp/decision.rs              # stable facade
crates/assay-core/src/mcp/decision_next/
  mod.rs
  event_types.rs
  builder.rs
  emitters.rs
  guard.rs
  normalization.rs
```

Step2 keeps the existing inline `#[cfg(test)]` block in `decision.rs`.
That keeps test selectors and reviewer expectations stable while the runtime
implementation moves behind the facade.

Current internal contract modules may remain where they are during Step2:
- `consumer_contract`
- `context_contract`
- `deny_convergence`
- `outcome_convergence`
- `replay_compat`
- `replay_diff`

The point of Step2 is not to redesign those modules; it is to reduce the main facade file and make
ownership clearer.

## Frozen mechanical boundaries
The intended mechanical split boundaries are:

- `event_types.rs`
  - decision/event/data structs and enums
- `builder.rs`
  - event construction helpers such as allow/deny/error builder flow
- `emitters.rs`
  - `DecisionEmitter`, `FileDecisionEmitter`, `NullDecisionEmitter`
- `guard.rs`
  - `DecisionEmitterGuard` lifecycle and single-emission invariant handling
- `normalization.rs`
  - obligation normalization, fulfillment path classification, projection refresh

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `decision.rs` remains the stable top-level facade
2. public decision/event surfaces remain compatible
3. event payload shape is unchanged
4. reason-code constants are unchanged
5. replay/contract refresh behavior is unchanged
6. tool-call handler, policy engine, CLI, and MCP server behavior are untouched

## Scope boundaries
### In scope
- freeze of the decision-kernel split plan
- explicit module-boundary proposal
- explicit public-surface freeze
- reviewer gates for the future mechanical split

### Out of scope
- implementation moves in `crates/assay-core/src/mcp/decision.rs`
- test rewrites in `crates/assay-core/tests/decision_emit_invariant.rs`
- policy engine changes
- tool-call handler changes
- CLI or MCP server changes
- workflow edits

## Planned wave structure
### Step1
Docs + gate only

### Step2
Mechanical split only:
- introduce `decision_next/`
- move bodies 1:1 behind a stable facade
- keep current contract modules semantically unchanged
- keep inline unit tests in `decision.rs`

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain decision-kernel split planning only.

Primary failure modes:
- sneaking behavior changes into a mechanical split
- changing event payload shape while chasing file size
- renaming reason codes or replay semantics under a refactor label
- expanding scope into handler/policy/server work
