# SPLIT REVIEW PACK - Wave43 Decision Kernel Step2

## Reviewer intent
Read this PR as a mechanical split of `mcp/decision.rs`, not as a redesign.

## What should look unchanged
- emitted decision payload shape
- reason-code strings
- replay/contract projection behavior
- public facade names used by handler/proxy/tests
- external tests under `crates/assay-core/tests/**`

## What should look moved
- core decision/event/data types
- fulfillment normalization helpers
- event builder methods
- emitter implementations
- guard lifecycle and drop safety net

## Read order
1. `crates/assay-core/src/mcp/decision.rs`
2. `crates/assay-core/src/mcp/decision_next/event_types.rs`
3. `crates/assay-core/src/mcp/decision_next/normalization.rs`
4. `crates/assay-core/src/mcp/decision_next/builder.rs`
5. `crates/assay-core/src/mcp/decision_next/emitters.rs`
6. `crates/assay-core/src/mcp/decision_next/guard.rs`
7. `scripts/ci/review-wave43-decision-kernel-step2.sh`

## Review questions
- Does `decision.rs` still act as the stable public facade?
- Are all runtime changes explainable as 1:1 moves into `decision_next/*`?
- Do the existing decision/replay tests still validate the same contracts?
- Is there any hidden payload, reason-code, or replay drift in the moved code?
