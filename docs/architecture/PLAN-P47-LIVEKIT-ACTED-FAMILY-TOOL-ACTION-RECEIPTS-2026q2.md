# PLAN - P47 LiveKit Acted-Family Tool Action Receipts (2026 Q2)

- **Date:** 2026-05-09
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the LiveKit acted-family candidate, add a small
  placeholder sample, and keep outreach notes separate from the technical spec.
  No public receipt-family matrix entry, no Trust Basis claim, and no production
  reducer are added in this slice.

## 1. One-line position

LiveKit is the first strong candidate for the acted-family: a bounded receipt
over what a voice agent actually invoked during a test or runtime turn.

Existing released families answer:

| Family | First surface | Question |
|---|---|---|
| tested | Promptfoo | What was tested? |
| decided | OpenFeature | What was decided? |
| built-with | CycloneDX | What was the system built with? |
| acted | LiveKit candidate | What did the agent actually invoke? |

This is not a fourth external-outcome family. It is the first runtime
capability-family candidate.

## 2. Why LiveKit

LiveKit Agents exposes a small event close to the acted-family boundary:
`FunctionToolsExecutedEvent`.

The LiveKit events documentation says `FunctionToolsExecutedEvent` is emitted
after all function tools have been executed for a given user input. The event
carries:

- `function_calls`
- `function_call_outputs`
- `has_tool_reply`
- `has_agent_handoff`

The JavaScript reference also exposes the event as
`type: "function_tools_executed"` with `functionCalls`,
`functionCallOutputs`, and `createdAt`.

The Python API shape allows `FunctionCallOutput | None`, which matters for
cancelled or non-returning calls. That gives Assay a clean acted-family seam
without importing transcripts, audio, room state, session usage, or full traces.

## 3. Relationship to existing P16

The repo already has:

- `docs/architecture/PLAN-P16-LIVEKIT-AGENTS-TESTING-RESULT-RUNEVENT-EVIDENCE-2026q2.md`
- `examples/livekit-runresult-evidence/`

P16 is a testing-result / `RunResult.events` lane. It includes messages,
function calls, function-call outputs, and handoffs.

P47 is narrower:

- P16: testing-result event list
- P47: acted-family tool execution receipts

P47 is the preferred execution candidate for acted-family work. P16 remains
historical/testing-result context and should not be widened in parallel for the
same family.

## 4. Proposed v1 seam

Input:

```text
one frozen Assay-side export shape derived from one LiveKit FunctionToolsExecutedEvent
```

Output:

```text
one or more reduced tool-action placeholder envelopes
```

Reduction unit:

```text
one function call + function call output pair
```

The importer may mirror LiveKit's conceptual `zipped()` pairing, but v1 receipt
generation is defined by the observed call/output lists and the pairing rules
below. It must not depend on helper methods from one SDK surface as the
semantic contract.

## 5. Frozen input shape

The sample uses an Assay-side frozen export shape. It is not a claim that
LiveKit provides this exact wire contract.

```json
{
  "schema": "livekit.function-tools-executed.export.v1",
  "framework": "livekit_agents",
  "surface": "function_tools_executed",
  "runtime_mode": "agent_session",
  "event_ref": "turn-42:function_tools_executed:0",
  "created_at": 1778320801.5,
  "function_calls": [
    {
      "id": "item_call_lookup_order",
      "call_id": "call_lookup_order_01",
      "name": "lookup_customer_order",
      "arguments": {
        "order_id": "ord_123",
        "include_items": true
      },
      "created_at": 1778320801.234,
      "group_id": null
    }
  ],
  "function_call_outputs": [
    {
      "id": "item_output_lookup_order",
      "call_id": "call_lookup_order_01",
      "name": "lookup_customer_order",
      "is_error": false,
      "output": {
        "status": "shipped",
        "items_count": 2
      },
      "created_at": 1778320801.467
    }
  ],
  "has_tool_reply": true,
  "has_agent_handoff": false
}
```

Optional event context may be included when naturally serialized:

- `has_tool_reply`
- `has_agent_handoff`

These fields are useful reviewer context, not proof that a reply or handoff was
correct.

## 6. Draft future receipt payload

Possible future receipt type:

```text
assay.receipt.livekit.tool-action.v1
```

Draft payload:

```json
{
  "schema": "assay.receipt.livekit.tool-action.v1",
  "source_system": "livekit_agents",
  "source_surface": "function_tools_executed",
  "source_artifact_ref": "examples/livekit-tool-action-evidence/fixtures/valid.livekit.json",
  "source_artifact_digest": "sha256:12cfa6055d536c2df588d86f031486f6b500488305f2434e8dcb655dc298b1b6",
  "reducer_version": "assay-livekit-function-tools-executed@0.1.0",
  "imported_at": "2026-05-09T10:00:02.000Z",
  "function": {
    "event_ref": "turn-42:function_tools_executed:0",
    "call_index": 0,
    "call_id": "call_lookup_order_01",
    "name": "lookup_customer_order",
    "arguments_hash": "sha256:a345c3f6b6bcff106507eff4c64e36c0126c0e7a6f6805ee0627235a82cd725d",
    "created_at": "2026-05-09T10:00:01.234000Z"
  },
  "outcome": {
    "completed": true,
    "is_error": false,
    "output_hash": "sha256:3500404e4cf7a4baac059eca3074bf50208cab017d4f027575a483a2cc12ba9b",
    "received_at": "2026-05-09T10:00:01.467000Z"
  },
  "event_context": {
    "event_created_at": "2026-05-09T10:00:01.500000Z",
    "has_tool_reply": true,
    "has_agent_handoff": false
  }
}
```

The hash values above are generated by the checked-in placeholder mapper and
can be recomputed from `examples/livekit-tool-action-evidence/fixtures/`.

## 7. Included fields

Required:

- `schema`
- `source_system`
- `source_surface`
- `source_artifact_ref`
- `source_artifact_digest`
- `reducer_version`
- `imported_at`
- `function.event_ref`
- `function.call_index`
- `function.name`
- `outcome.completed`
- `event_context.event_created_at`

Optional when naturally present:

- `function.call_id`
- `function.group_id`
- `function.arguments_hash`
- `outcome.is_error`
- `outcome.output_hash`
- `outcome.received_at`
- `event_context.has_tool_reply`
- `event_context.has_agent_handoff`
- `capture_context.session_id`

Out of scope by default:

- `subject_ref`

`subject_ref` may become an adapter-supplied reviewer aid later, but it should
not be part of the canonical v1 receipt unless a naturally bounded source field
is demonstrated in real captured input. Otherwise it risks smuggling
application/business-object truth into an execution receipt.

## 8. Excluded fields

V1 must exclude:

- raw transcript
- raw audio
- raw user input
- raw model output
- raw tool arguments
- raw tool output
- session usage
- latency metrics
- room state
- participant identity
- full trace/span payloads
- full LiveKit event wrapper

## 9. Decision boundary

LiveKit's event tells us that tool execution happened.

It does not by itself tell us:

- whether the tool call was allowed by policy
- whether the tool call was correct
- whether the user intended it
- whether the voice interaction succeeded
- whether the handoff was semantically correct

So v1 should not include `decision` unless there is a separate declared-intent
or policy-template sidecar.

Recommended split:

```text
LiveKit event -> observed action receipt
policy template -> declared intent
future CI diff -> expected vs observed
```

Timestamp boundary:

- `created_at` is surfaced LiveKit event/call/output time when naturally
  present.
- `imported_at` is Assay receipt provenance time.
- They must remain distinct and must not be collapsed for convenience.

## 10. Reducer rules

1. Require `type == "function_tools_executed"` when present.
2. Require a non-empty `function_calls` list.
3. Require `function_call_outputs` to pair cleanly with calls.
4. Prefer pairing by `call_id` when present on both sides.
5. Fall back to list order only when `call_id` is absent.
6. Reject mismatched call/output counts in v1.
7. Treat a missing output for any included call as malformed in the placeholder
   sample.
8. Do not introduce a `missing_output` status yet.
9. Require stable non-empty function names.
10. Derive runtime status from `is_error` when output is present.
11. Do not copy raw arguments or raw output into the receipt.
12. If raw arguments/output are available, store only digest/ref.
13. Copy `has_tool_reply` and `has_agent_handoff` as optional event context,
    not as proof that a reply or handoff was correct.
14. Reject transcript/audio/usage fields if they appear in the reduced input.

Note: LiveKit Python permits `FunctionCallOutput | None`. A future production
reducer may model this as `completed=false`, but this placeholder slice keeps
the v1 fixture stricter until a real captured input requires the richer status.

## 11. Non-claims

Assay does not claim:

- LiveKit tool behavior was correct
- the user intended the action
- the voice session succeeded
- the agent completed the task
- LiveKit's full runtime state was imported
- the transcript/audio/room state is preserved
- the policy template is correct

Assay only claims:

```text
this bounded LiveKit tool-action observation was reduced, bundled, and made
reviewable under this receipt boundary
```

## 12. Recommended next PR after this slice

This slice should land first as docs/fixtures only.

After review, a later implementation PR may:

1. Add a real receipt schema.
2. Add an `assay evidence import livekit-function-tools` command.
3. Add end-to-end fixture tests.
4. Add a receipt-family matrix entry only after the acted-family shape is
   fixture-proven.
5. Add a Trust Basis claim only in a separate, explicitly reviewed claim slice.

## 13. Open review questions

1. Should v1 emit one receipt per tool call, or one receipt per
   `FunctionToolsExecutedEvent` batch?
2. Should `subject_ref` stay fully out of v1, or be allowed only as a
   non-canonical adapter-supplied reviewer aid?
3. Should P16 remain historical/testing context, or be renamed to make the
   acted-family distinction more visible?
4. Should `has_tool_reply` and `has_agent_handoff` be copied to every receipt,
   or emitted once as event-level context in a batch receipt?
5. Is `ok|error` enough for v1, with missing output treated as malformed in
   this placeholder slice, or do we need richer status values immediately?

## 14. Defaults

1. One receipt per tool call.
2. Keep `subject_ref` out of the canonical v1 receipt.
3. Leave P16 alone for now, but document P47 as the acted-family execution
   successor.
4. Copy event context into each receipt for review simplicity.
5. Start with completed/error only; add richer statuses only after a real
   fixture needs them.

## References

- LiveKit Agents events docs:
  https://docs.livekit.io/reference/agents/events/
- LiveKit JS `FunctionToolsExecutedEvent` reference:
  https://docs.livekit.io/reference/agents-js/types/agents.voice.FunctionToolsExecutedEvent.html
- LiveKit Python API docs:
  https://docs.livekit.io/reference/python/livekit/agents/index.html
- Existing Assay P16 plan:
  `docs/architecture/PLAN-P16-LIVEKIT-AGENTS-TESTING-RESULT-RUNEVENT-EVIDENCE-2026q2.md`
- Existing Assay LiveKit sample:
  `examples/livekit-runresult-evidence/`
