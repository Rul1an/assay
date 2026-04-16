# OpenAI Agents JS Tool Approval Interruption Evidence Sample

This example turns one tiny frozen artifact derived from a paused OpenAI
Agents JS approval run into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start from one paused approval run only
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the two good artifacts into Assay-shaped placeholder envelopes
- keep `interruptions` plus one resumable continuation anchor at the center
- keep transcript, session, provider-chaining, and full serialized `RunState`
  truth out of Assay

## What is in here

- `map_to_assay.py`: turns one tiny OpenAI Agents JS approval-interruption
  artifact into an Assay-shaped placeholder envelope
- `fixtures/valid.openai-agents-js.json`: one bounded paused approval sample
- `fixtures/failure.openai-agents-js.json`: one weaker but still valid paused
  approval sample
- `fixtures/malformed.openai-agents-js.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats one paused approval run plus one resumable continuation
anchor as the best first OpenAI Agents JS lane hypothesis for Assay.

That keeps the first slice on bounded interruption evidence only. It does not
turn the sample into:

- a transcript import path
- a session import path
- a `previousResponseId` / `lastResponseId` lane
- a full `newItems` lane
- a full serialized `RunState` lane

The checked-in fixtures are deliberately smaller than the full SDK runtime.
OpenAI Agents JS itself is broader than this sample. V1 keeps the evidence
boundary honest without pretending that Assay now models OpenAI Agents JS
result or continuation semantics as a whole.

The top-level `schema`, `framework`, and `surface` fields in these fixtures
are sample wrapper metadata. They help identify the frozen artifact and the
seam hypothesis, but they are not a claim that OpenAI Agents JS itself ships
one canonical wrapper with those same labels.

## Current discovery seam

This sample is grounded in one small runtime-backed local probe against the
public `@openai/agents` package.

What that means in practice:

- the public package version was `0.8.3`
- the run used one top-level agent and one local function tool with
  `needsApproval: true`
- the model path was a tiny fake model, not a live provider
- the first run paused and returned one real `interruptions` item
- the paused `state` was serialized and restored with
  `RunState.fromString(...)`
- the same paused run then resumed after `approve(...)`

The current runtime-backed interruption looked like this at the boundary we
care about:

- tool name: `send_email`
- outer agent name: `P22ApprovalProbe`
- the top-level interruption item exposed `toolName` and `agent`
- the call id was visible under `interruption.rawItem.callId`, not as a
  top-level interruption property

That last point is important for the sample shape.

In this sample, `call_id_ref` is a bounded Assay-side reduction over the live
interruption object. It is not a claim that the interruption item itself
publishes one canonical top-level `callId` field.

The current runtime-backed serialized state had:

- length `3782`
- SHA-256
  `a136d3d331cff5810ec27c7afc5fed9b0e16ed8608e5e698358eedbffb83fd51`

So the checked-in valid fixture uses:

- `resume_state_ref = runstate:sha256:a136d3d331cff5810ec27c7afc5fed9b0e16ed8608e5e698358eedbffb83fd51`

Important honesty line:

- the paused-run runtime path is real
- the current provider-live path is **not** yet proven

This probe does not use a live model provider, a session, or
`previousResponseId` chaining. So the checked-in fixtures are runtime-backed
but still pre-proof on provider-backed continuation behavior.

## Why the interruption subset is smaller than a UI payload

The public docs and examples show that approval UIs can inspect richer
interruption content, including arguments or raw payloads.

This sample deliberately keeps `interruptions` smaller than that.

For v1, each interruption only keeps:

- `tool_name`
- `call_id_ref`
- optional `agent_ref`

It does **not** import:

- tool arguments
- rejection text
- raw interruption payloads
- full approval UI state

That keeps the lane on pending approval evidence first, not on approval UI
design or resolved outcome truth.

## Map the checked-in valid artifact

```bash
python3 examples/openai-agents-js-approval-interruption-evidence/map_to_assay.py \
  examples/openai-agents-js-approval-interruption-evidence/fixtures/valid.openai-agents-js.json \
  --output examples/openai-agents-js-approval-interruption-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-16T12:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/openai-agents-js-approval-interruption-evidence/map_to_assay.py \
  examples/openai-agents-js-approval-interruption-evidence/fixtures/failure.openai-agents-js.json \
  --output examples/openai-agents-js-approval-interruption-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-16T12:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/openai-agents-js-approval-interruption-evidence/map_to_assay.py \
  examples/openai-agents-js-approval-interruption-evidence/fixtures/malformed.openai-agents-js.json \
  --output /tmp/openai-agents-js-approval-malformed.assay.ndjson \
  --import-time 2026-04-16T12:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture smuggles a
top-level `history` transcript into a lane that intentionally stays smaller
than transcript truth.

That is an explicit product-boundary rejection, not just parser hygiene:

- this lane is not the full `history` surface
- a top-level transcript would silently widen the claim surface
- if we want a transcript lane later, that should be a different lane or a
  future explicitly narrowed slice

The same rule applies to other continuation drift. For v1, artifacts that mix
`history`, `session`, `lastResponseId`-style hints, or raw serialized
`RunState` into the same lane should be treated as malformed rather than
partially imported.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat transcript truth, session truth, or provider-managed continuation
  truth as Assay truth
- imply that Assay independently verified broad approval semantics
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest paused approval artifact, not OpenAI
Agents JS as a whole.

Additional caps in this sample:

- `pause_reason` must stay `tool_approval`
- `interruptions` must stay a non-empty bounded list
- `tool_name` stays on a small classifier surface
- `resume_state_ref` must stay an opaque id, not a raw serialized blob or URL
- optional refs such as `active_agent_ref`, `last_agent_ref`, and `agent_ref`
  must stay opaque ids

We are not asking Assay to inherit OpenAI Agents JS transcript, session,
provider-chaining, or full `RunState` semantics as truth.

## Checked-in fixtures

- `fixtures/valid.openai-agents-js.json`: bounded paused approval sample
- `fixtures/failure.openai-agents-js.json`: weaker but still valid paused
  approval sample
- `fixtures/malformed.openai-agents-js.json`: malformed import case that
  wrongly pulls transcript history into the lane
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
