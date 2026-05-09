# P47 LiveKit Outreach Notes

- **Status:** Companion note, not part of the technical spec.
- **Scope:** Shape-feedback outreach for the proposed LiveKit acted-family lane.
- **Non-goal:** Partnership, endorsement, or LiveKit-side roadmap commitment.

## Target

Repository: `livekit/agents`

Best channel:

1. LiveKit community forum first.
2. GitHub issue only if maintainers point there or if a concrete SDK or
   documentation gap is found.

Forum responses are best-effort community support, not guaranteed maintainer
review. Treat any answer as shape feedback, not as LiveKit endorsement.

Do not DM maintainers first. The ask is technical shape feedback and benefits
from public scrutiny.

## What to ask

Two questions only:

1. Did we read `FunctionToolsExecutedEvent` correctly?
   - Pair by `call_id` when present.
   - Fall back to order only when `call_id` is absent.
   - Treat mismatched list lengths as malformed in the first Assay-side
     importer-only slice.
2. Is serializing session events into a small `events.ndjson` capture something
   LiveKit users already do, or would adoption require a helper example?

Do not ask for:

- a LiveKit integration
- endorsement
- docs placement
- a roadmap commitment
- a stable LiveKit wire-format guarantee

## Draft message

Title:

```text
Question: bounded evidence receipts from FunctionToolsExecutedEvent
```

Body:

```text
Hi LiveKit Agents folks,

I'm working on a small Assay-side evidence reducer for agent test/runtime
artifacts. The goal is narrow: take one bounded `FunctionToolsExecutedEvent`
capture and reduce it into reviewable tool-action receipts.

No LiveKit integration ask, no endorsement ask, and no request for a roadmap
commitment. I just want to make sure the shape interpretation is sane before
treating this importer-only path as a durable acted-family candidate.

The proposed boundary is:

- input: one serialized `FunctionToolsExecutedEvent`
- output: one receipt per observed function tool call
- include: function name, call_id when present, argument/output hashes,
  completed/error state, event/call/output timestamps
- exclude: raw arguments, raw output, transcript, audio, room state, usage, and
  traces

Two questions where I'd value your read:

1. Is pairing by `call_id` the right first rule, with order fallback only when
   IDs are absent? Are there retry/cancel/parallel cases where this breaks?

2. Do LiveKit users commonly serialize session events to a file/stream for
   offline review, or would a small event-capture helper be needed for this to
   be useful?

If this is off or too narrow, happy to be corrected. The intent is to consume a
published event shape carefully, not to make LiveKit own an evidence standard.
```

## Single-shot policy

Post once. Do not bump.

If there is no response after 14 days, it is acceptable to continue from the
published event shape, as long as the Assay docs keep saying:

- proposed
- Assay-side frozen export shape
- no LiveKit endorsement
- no LiveKit wire-contract claim

## Success criteria

Any of these count as success:

- A maintainer confirms or corrects the pairing model.
- A LiveKit user says the event-capture shape would be useful.
- No response after 14 days, but the implementation remains aligned with the
  public docs and preserves the non-endorsement boundary.

This outreach does not need:

- a partnership announcement
- a co-marketing moment
- LiveKit-side code changes
- inclusion in LiveKit docs

## Timing

Post only after the P47 Stage 1 importer and fixtures are merged into the Assay
repo, so the discussion can link to stable source files rather than `/tmp`
drafts.
