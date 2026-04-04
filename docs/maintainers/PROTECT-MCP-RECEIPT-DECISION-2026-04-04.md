# Protect MCP Receipt Decision Note

Date: 2026-04-04

## Decision

Keep the ScopeBlind / VeritasActa signed-receipt line at **probe status**.

Do not open a product wave, schema freeze, public roadmap item, pack, or Trust Card extension from this yet.

## Why

The corrected second probe materially improved confidence:

- Assay can ingest the bounded receipt shape cleanly in a test-only harness
- existing MCP import behavior remains unaffected
- the corrected Passport-envelope samples verify under a pinned external boundary

That is enough to say the surface is credible.

It is not enough to say the surface is ready.

## What is still missing

The remaining blockers are contract quality, not basic feasibility:

- timestamp semantics are still claim-shaped
- `tool_input_hash` is still not strong enough for hard reasoning without a tighter interop story
- binding identity still needs a frozen statement of assumptions
- malformed and key-handling behavior are still too young to treat as stable public boundary semantics

## Maintainer call

Current call:

- keep exploring in discussion if useful
- allow future bounded probes
- do not imply adoption
- do not imply trust promotion
- do not imply that a real Assay evidence seam has been accepted

If this line moves again, the next acceptable step is a tighter contract pass, not implementation.
