# Paused HITL Evidence Pattern

This reference pattern defines the smallest honest Assay-side shape for
paused human-in-the-loop approval evidence.

It exists to keep approval-aware evidence artifacts:

- bounded
- reviewable
- reusable across runtimes
- smaller than the runtime that first motivated the pattern

This pattern was first internalized from the confirmed `P22` OpenAI Agents JS
lane, but it is intentionally documented as an Assay-side reference shape, not
as OpenAI Agents JS support in general.

## What This Pattern Is For

Use this pattern when a runtime:

- pauses on a pending human approval step,
- exposes a bounded interruption surface,
- and provides enough pause-state material for Assay to derive one
  continuation anchor.

This pattern is for:

- one paused approval artifact
- one bounded `interruptions` list
- one derived `resume_state_ref`

## What This Pattern Is Not For

This pattern is not for:

- transcript truth
- session truth
- provider-managed continuation truth
- full serialized state truth
- resumed outcome truth
- dashboard or reviewer UI state

Normative framing:

> Assay claims only bounded paused approval evidence plus one derived
> continuation anchor from the same paused run. It does not claim transcript
> truth, session truth, provider-managed continuation truth, full serialized
> state truth, or resolved approval outcome truth.

## Canonical Fields

### Required

- `schema`
- `framework`
- `surface`
- `pause_reason`
- `interruptions`
- `resume_state_ref`
- `timestamp`

### Optional

- `active_agent_ref`
- `last_agent_ref`
- `metadata_ref`

### Not Allowed In v1

- raw serialized state
- transcript history
- session identifiers
- `newItems`
- provider continuation fields
- resumed decision fields
- `approve` / `reject` outcome semantics

## Canonical Names

### `pause_reason`

For v1, the only allowed value is:

- `tool_approval`

This keeps the pattern tied to the confirmed paused approval seam instead of
quietly widening into every resumable runtime state.

### `interruptions`

This is the bounded observed list of pending approval items.

Each item should stay on the smallest honest subset:

- `tool_name`
- `call_id_ref`

Weak reviewer aids may be added only when they are naturally present and do not
recast the interruption item as broader runtime truth.

### `call_id_ref`

This is an opaque bounded anchor for the interrupted call.

It must remain:

- short
- reviewable
- non-resolving

It must not become:

- a call payload
- argument/body truth
- replay guarantee

### `resume_state_ref`

This is the canonical Assay-side name for the derived continuation anchor.

It must always be documented as:

- Assay-local derived fingerprint
- derived from serialized paused state
- not a native runtime field

It must not be described as:

- a raw state export
- a portable byte-stable wire identity
- a resolver URL

## Observed Vs Derived

This distinction is the heart of the pattern.

### Observed

Observed fields come from the runtime seam itself.

For this pattern, that includes:

- a paused approval state exists
- an interruption list exists
- interruption item fields actually present on that paused seam

### Derived

Derived fields are produced by Assay from runtime-supporting material.

For this pattern, that includes:

- `resume_state_ref`

Important warning:

> Observed interruption surface is not the same thing as derived resumability
> anchor. Do not promote derived continuation support into native runtime
> truth.

## Compact Valid Example

```json
{
  "schema": "assay.harness.approval-interruption.v1",
  "framework": "openai_agents_sdk",
  "surface": "tool_approval",
  "pause_reason": "tool_approval",
  "interruptions": [
    {
      "tool_name": "dangerous_write",
      "call_id_ref": "call_12345"
    }
  ],
  "resume_state_ref": "sha256:9d59fd2e8c87f4a08d6f8db641bb37cb34abfb40c0a6c0f73b43cc3c51dca0d9",
  "timestamp": "2026-04-18T12:00:00Z"
}
```

## Malformed Conditions

Treat artifacts as malformed if they contain:

- missing `interruptions`
- empty `interruptions`
- `pause_reason != tool_approval`
- raw serialized state inline
- transcript/history fields
- full rich result arrays
- `resume_state_ref` as a URL
- resumed decision fields in a pause-only artifact

## Why This Pattern Matters

This pattern gives Assay one reusable, explicit posture for paused HITL
evidence:

- the interruption surface stays observed
- the continuation anchor stays derived
- the artifact stays smaller than runtime truth

That is the key discipline. It keeps Assay useful for pause-aware evidence
without turning Assay into a continuation runtime.
