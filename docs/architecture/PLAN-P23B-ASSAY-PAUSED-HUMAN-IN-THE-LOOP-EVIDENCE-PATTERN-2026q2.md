# PLAN — P23B Assay Paused Human-in-the-Loop Evidence Pattern (2026 Q2)

- **Date:** 2026-04-18
- **Owner:** Evidence / Docs
- **Status:** Planning lane
- **Scope (current repo state):** Internalize the confirmed `P22` seam into
  Assay itself as a reference evidence pattern, naming guide, and contract
  boundary for paused human-in-the-loop artifacts. This plan is for Assay-side
  evidence guidance and validation posture only. It does **not** propose
  runtime helper implementation, full adapter support, session/history support,
  or broad continuation semantics.

## 1. Why this plan exists

Assay now has enough confirmation to treat paused human-in-the-loop approval as
a reusable evidence pattern, not just a one-off lane.

`P22` proved a particularly useful Assay truth boundary:

- one observed interruption surface
- one derived continuation anchor
- no need to import raw runtime state as evidence truth

That makes it a strong first internalized reference pattern for Assay itself.

## 2. What this plan is and is not

This plan is for:

- canonical naming guidance
- observed-vs-derived guidance
- required/optional field boundaries
- malformed rules
- a small reference pattern document

This plan is not for:

- a new event framework
- full runtime adapter support
- broad continuation semantics
- session/history support
- resumed outcome semantics
- dashboards or reviewer UI
- runtime helper implementation

## 3. Hard positioning rule

Assay must not turn one approved seam into a broad runtime claim.

Normative framing:

> `P23B` v1 claims only bounded paused approval evidence plus one derived
> continuation anchor from the same paused run. It does not claim transcript
> truth, session truth, provider-managed continuation truth, full serialized
> state truth, or resolved approval outcome truth.

That means:

- `interruptions` are observed runtime surface
- `resume_state_ref` is derived evidence support material
- Assay remains an evidence compiler, not a continuation runtime

## 4. Canonical Assay naming guidance

The reference pattern should standardize these names:

- `pause_reason`
- `interruptions`
- `call_id_ref`
- `resume_state_ref`

### 4.1 `pause_reason`

For v1, the only allowed value is:

- `tool_approval`

This keeps the pattern tied to the confirmed human-in-the-loop seam.

### 4.2 `interruptions`

This is the bounded observed list of pending approval items.

Each item should stay on the smallest honest subset:

- `tool_name`
- `call_id_ref`
- optional weak reviewer-aid fields only when naturally present

### 4.3 `call_id_ref`

This is an opaque bounded anchor for the interrupted call.

It must remain:

- short
- reviewable
- non-resolving

It must not become:

- a call payload
- argument/body truth
- a replay guarantee

### 4.4 `resume_state_ref`

This is the canonical Assay-side name for the derived continuation anchor.

It must always be documented as:

- Assay-local derived fingerprint
- derived from serialized paused state
- not a native runtime field

## 5. Observed vs derived guidance

This distinction is the heart of the pattern.

### 5.1 Observed

Observed fields are surfaced by the runtime or protocol seam itself.

For this pattern, that includes:

- pause state exists
- interruption list exists
- interruption item fields actually present

### 5.2 Derived

Derived fields are produced by Assay from runtime-supporting material.

For this pattern, that includes:

- `resume_state_ref`

The reference document must explicitly warn:

> Observed interruption surface is not the same thing as derived resumability
> anchor. Do not promote derived continuation support into native runtime
> truth.

## 6. Reference artifact contract

The Assay-side reference contract should be:

Required:

- `schema`
- `framework`
- `surface`
- `pause_reason`
- `interruptions`
- `resume_state_ref`
- `timestamp`

Optional:

- `active_agent_ref`
- `last_agent_ref`
- `metadata_ref`

Not allowed in v1:

- raw serialized state
- transcript history
- session identifiers
- `newItems`
- resumed decision data
- provider continuation fields
- `approve`/`reject` outcome semantics

## 7. Malformed rules

The pattern should define these malformed conditions:

- missing `interruptions`
- empty `interruptions`
- `pause_reason != tool_approval`
- raw serialized state added inline
- full transcript/history included
- full rich result arrays included
- `resume_state_ref` given as URL
- resumed decision fields included in a pause-only artifact

## 8. Documentation deliverables

Suggested file:

```text
docs/
  reference/
    patterns/
      paused-hitl-evidence.md
```

This document should include:

- what this pattern is for
- what it is not for
- required/optional fields
- observed-vs-derived examples
- malformed examples
- one compact valid example

## 9. Example deliverables

Suggested example area:

```text
examples/
  approval-interruption-evidence/
    README.md
    map_to_assay.py
    fixtures/
      valid.openai-agents-js.json
      failure.openai-agents-js.json
      malformed.openai-agents-js.json
      valid.assay.ndjson
      failure.assay.ndjson
```

This example should teach:

- pause-only artifact discipline
- no raw state in canonical evidence
- derived continuation anchor handling
- malformed fast-fail behavior

## 10. Implementation phases

### Phase A — Reference doc

Deliverables:

- paused HITL reference document
- naming guidance
- observed-vs-derived section
- malformed rules

Acceptance:

- pattern is readable without lane-specific context
- no broad runtime claim leaks in

### Phase B — Example tightening

Deliverables:

- example README
- example fixtures
- example outputs
- validator behavior aligned with the pattern

Acceptance:

- example and docs say the same thing
- malformed cases reflect pattern drift, not parser quirks only

### Phase C — Validation posture

Deliverables:

- explicit validation checks for the pause-only pattern
- docs note on why raw state is excluded
- docs note on why derived anchors are still useful

Acceptance:

- pattern is enforceable
- pattern remains smaller than runtime truth

## 11. Success criteria

This plan succeeds when:

- Assay has one clear paused HITL reference pattern
- the names are stable and reusable
- observed-vs-derived continuation guidance is explicit
- future lanes can reuse this pattern without re-litigating the core shape

## 12. Final judgment

`P23B` should make paused human-in-the-loop evidence a first-class Assay
reference pattern:

- one observed interruption surface
- one derived continuation anchor
- and no broader runtime-truth import
