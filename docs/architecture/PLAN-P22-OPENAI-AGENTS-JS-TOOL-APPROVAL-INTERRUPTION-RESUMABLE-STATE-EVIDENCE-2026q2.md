# PLAN — P22 OpenAI Agents JS Tool Approval Interruption / Resumable-State Evidence Interop (2026 Q2)

- **Date:** 2026-04-16
- **Owner:** Evidence / Product
- **Status:** Planning lane with one runtime-backed local probe
- **Scope (current repo state):** Define one bounded OpenAI Agents
  JS-adjacent lane centered on approval interruptions plus one resumable
  continuation anchor derived from the same paused run. This plan does **not**
  propose broad OpenAI Agents JS support, transcript import, session truth,
  server-managed continuation truth, or full `RunState` import.

## 1. Why `P22` should exist

`openai/openai-agents-js` is now a real, active upstream with a clear public
shape around:

- agent runs
- result surfaces
- human-in-the-loop approvals
- sessions
- tracing

That matters because Assay does **not** need the whole SDK.

It needs the smallest honest external-consumer seam that:

- already exists in named public docs,
- is small enough to review without inheriting SDK runtime truth,
- and has real live design pressure in the public repo.

The strongest candidate seam is:

- one paused `RunResult`
- one bounded `interruptions` list
- one resumable continuation anchor derived from the same `RunState`

That lane is smaller than:

- full transcript `history`
- full `newItems`
- full `runContext`
- provider-managed `lastResponseId` chaining
- session lifecycle truth
- full serialized `RunState`
- raw model response or tracing truth

## 2. Why this is timely

This seam is already explicit in the public docs.

The Results guide says the right surface for:

- pending approvals,
- and a resumable snapshot,

is `interruptions` plus `state`.

The Human-in-the-loop guide then makes the behavior concrete:

- a tool call that needs approval pauses the run,
- the SDK returns `interruptions`,
- the caller resolves them on `result.state`,
- and the same paused run resumes from the same `RunState`.

This exact area is also alive upstream in public issues:

- `#1097` shows that approval-plus-resume behavior still raises compatibility
  questions across manual history, provider-managed continuation, and
  session-backed resume paths
- `#1104` shows that rejection signaling is still a living boundary question
  rather than a closed, stable truth surface

That is exactly the kind of upstream posture Assay can work with:

- the seam is named
- the docs are strong enough to start from
- the behavior is useful
- but the broader continuation semantics are still alive enough that we should
  stay small

## 3. Hard positioning rule

This lane must stay smaller than the upstream ecosystem name.

Normative framing:

> `P22` v1 claims only bounded approval-interruption evidence plus one
> resumable continuation anchor derived from a paused run. It does not claim
> transcript truth, session truth, server-managed continuation truth, or
> complete `RunState` truth.

That means:

- OpenAI Agents JS remains the runtime, not Assay truth
- a paused approval run is an observed upstream state, not the truth of the
  whole conversation lifecycle
- Assay stays an external evidence consumer, not an authority on persistence
  strategy, replay completeness, or approval outcome semantics

Common anti-overclaim sentence:

> We are not asking Assay to inherit OpenAI Agents JS transcript, session,
> provider-chaining, or `RunState` semantics as truth.

## 4. Why this seam and not sessions/results in general

The public docs expose many result surfaces:

- `finalOutput`
- `history`
- `output`
- `newItems`
- `lastAgent`
- `lastResponseId`
- `interruptions`
- `state`
- `runContext`

That is useful, but it is also exactly why the first wedge should be smaller
than "results interop."

The first honest wedge is:

- the run paused on approval,
- these are the pending approval items,
- and this paused run has one resumable continuation anchor.

That is more reviewable than:

- the whole conversation transcript,
- the whole rich run-item stream,
- or the whole serialized run-state blob.

## 5. Why not broader continuation surfaces

### 5.1 Why not `history`

`history` is explicitly the replay-ready next-turn input with the full local
transcript.

That makes it useful product-wise and wrong as the first Assay wedge.

Why:

- it is already transcript truth territory
- it widens into manual chat loop semantics
- the docs explicitly warn that mixing client-managed history with
  server-managed state can duplicate context

That is too broad.

### 5.2 Why not `session`

Sessions are important, but they are not the first seam.

The docs describe them as:

- fetching stored history before a turn,
- persisting new items after each run,
- and remaining available for future turns and interrupted resumes

That is lifecycle and memory-management truth, not a small evidence seam.

### 5.3 Why not `previousResponseId` / `lastResponseId`

Provider-managed continuation is real, but it is also provider-specific and
already adjacent to compatibility questions.

The docs say `lastResponseId` is the value to pass as `previousResponseId`
when you are using OpenAI Responses API chaining.

That is useful and still too broad for v1 because it opens immediately into:

- persistence strategy choice
- provider-managed state semantics
- mode-mixing questions with `history`, `session`, and `conversationId`

Issue `#1097` makes that exact compatibility pressure visible.

### 5.4 Why not full `newItems`

`newItems` is a strong surface and the docs explicitly position it as the rich
run-item view when agent/tool/handoff metadata matters.

But it is still broader than the first seam we want because it also carries:

- message items
- tool outputs
- handoff boundaries
- other run-item metadata

The first wedge should stay on:

- the approval interruption itself,
- not the whole rich run delta.

### 5.5 Why not full serialized `RunState`

The docs explicitly say `state` is the serializable snapshot behind the
result, and that you can serialize it and resume later.

That is useful and too rich to import wholesale into the first evidence
contract.

For Assay, the first lane should use:

- one bounded anchor derived from serialized state,

not:

- the whole serialized state blob as canonical evidence.

## 6. Upstream caveats we must preserve

The seam is good, but it is not closed.

Three cautions matter immediately.

### 6.1 Approval interruptions are run-wide

The docs are explicit that approval interruptions surface on the outer run,
including:

- direct tool approvals,
- handoff-reached tools,
- and nested `agent.asTool()` approvals

So `P22` must not pretend the seam is only about one local tool call in one
top-level agent.

The lane should still stay on the paused outer run artifact.

### 6.2 Rejection outcome semantics are still alive upstream

Issue `#1104` is the strongest public signal here.

The important lesson is not "rejections are broken forever."

The important lesson is:

- the structural signaling for rejected tool calls is still moving,
- and public discussion there makes it clear that apps should not treat the
  current `status` field as a settled rejection-truth surface.

So `P22` must not make rejection result semantics a required part of the first
sample contract.

### 6.3 Continuation-mode compatibility is still being investigated

Issue `#1097` is the strongest public signal here.

The public issue discussion makes it clear that the unresolved compatibility
boundary still spans:

- manual history
- `previousResponseId`
- `conversationId`
- session-backed resumes

So `P22` must stay smaller than "OpenAI Agents JS continuation support."

## 7. Recommended `P22` seam

The correct lane name is intentionally small:

**OpenAI Agents JS Tool Approval Interruption / Resumable-State Artifact Lane**

The recommended v1 seam is:

- one paused approval run envelope
- one bounded `interruptions` list
- one fixed pause reason tied to tool approval
- one resumable continuation anchor derived from the same paused `RunState`

This is **not**:

- session support
- transcript support
- full `newItems` support
- general `RunResult` support
- provider-chaining support
- full `RunState` import

Important framing rule:

> The first sample should use a frozen artifact derived from one paused
> approval run, not a claim that Assay models OpenAI Agents JS result or
> continuation semantics as a whole.

## 8. Recommended v1 artifact contract

Use one frozen serialized artifact derived from the approval-interruption
lane.

The first artifact should stay small and self-describing:

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

Important framing rule:

> The sample uses one frozen artifact derived from `RunResult.interruptions`
> and one resumable continuation anchor derived from `RunState`. It is not a
> claim that OpenAI Agents JS publishes one stable external evidence wrapper
> for approval or resume consumers.

## 8.1 Field meaning

### `pause_reason`

This is required.

In v1 the only allowed value should be:

- `tool_approval`

Why this is strict:

- it keeps the lane tied to the named public human-in-the-loop seam
- it prevents drift into generic pauses, retries, or stream aborts
- it keeps `P22` about approval interruptions instead of every resumable case

### `interruptions`

This is required.

It should stay:

- bounded
- ordered
- reviewable
- specific to pending tool approvals

It must not become:

- a full `newItems` dump
- raw `runContext`
- transcript history
- a whole approval UI state model

Each v1 interruption should stay on the smallest honest subset:

- `tool_name`
- `call_id_ref`
- optional `agent_ref`

Nothing else should enter the first sample unless one real implementation
forces it.

v1 does not import:

- tool arguments
- rejection text
- raw interruption payloads

into the canonical artifact.

### `resume_state_ref`

This is required.

It is an Assay-side bounded anchor derived from serialized `RunState`, not
evidence that OpenAI Agents JS publishes one native `resumeStateRef` field.

It must remain:

- opaque
- portable
- non-resolving

It must not become:

- a raw serialized `RunState` dump in the canonical contract
- a dashboard link
- a provider-specific continuation promise

It must also not be read as a claim that the underlying serialized
`RunState` object is:

- small
- stable
- protocol-agnostic
- or itself part of the evidence contract

### `active_agent_ref` / `last_agent_ref`

These are optional.

They are only useful if one real run proves they are naturally present and
help reviewability.

They must not become:

- a demand to encode full multi-agent control flow
- a claim about which agent will always own the next turn

## 9. Assay-side meaning

The `P22` sample may only claim bounded approval-interruption evidence.

Assay must not treat as truth:

- transcript truth
- session truth
- provider-managed continuation truth
- full serialized `RunState` truth
- rejection-outcome truth

The artifact only says:

- this run paused for tool approval
- these pending approval items were surfaced
- one resumable continuation anchor existed for the same paused run

## 10. Discovery gate before implementation

Do not build a sample from docs alone.

Before closing this lane, do one bounded discovery pass:

1. create one tiny OpenAI Agents JS harness
2. define one tool that requires approval
3. trigger exactly one paused approval run
4. capture the smallest honest `interruptions` shape
5. serialize the paused `state`
6. derive one bounded `resume_state_ref`
7. approve or reject through the same `state`
8. resume from the same paused run

Discovery is only done when we have:

- one real paused approval run
- one real `interruptions` payload
- one real serialized-state round-trip
- one explicit note about which fields were naturally present
- one explicit note about what we still refused to import

## 10.1 Current discovery seam

This lane is no longer docs-only.

One small runtime-backed local probe has now been run against the public
`@openai/agents` package using:

- one top-level agent
- one local function tool with `needsApproval: true`
- one fake local model that emitted one tool call
- one serialized-state round-trip through `RunState.toString()` and
  `RunState.fromString(...)`

What was observed in that probe:

- the first run paused and returned one real interruption item
- the interruption object naturally exposed:
  - `toolName`
  - `agent`
  - `rawItem`
- the call id was visible under `rawItem.callId`, not as a first-class
  top-level interruption property
- the serialized paused state length was `3782`
- the serialized paused state SHA-256 was
  `a136d3d331cff5810ec27c7afc5fed9b0e16ed8608e5e698358eedbffb83fd51`
- resuming from the same serialized state after `approve(...)` produced a
  final output and zero remaining interruptions

That runtime result strengthens the lane in two ways:

- `resume_state_ref` can now be grounded in one real serialized-state anchor
- `call_id_ref` is now more honestly framed as an Assay-side bounded reduction
  over live interruption data, because the current interruption object does not
  surface a top-level `callId`

Important honesty line:

- the paused-run runtime path is real
- provider-backed continuation behavior is still not proven by this first probe

## 10.2 Exit criterion for `P22`

`P22` is not closed just because a plan exists.

This lane is only complete when all of the following are true:

- one real approval interruption has been captured from a runnable OpenAI
  Agents JS setup
- the current required vs optional field split has been checked against that
  run
- `resume_state_ref` has been derived from a real serialized `RunState`
- the paused run has actually resumed from the same serialized state
- the sample still stays smaller than transcript, session, and provider-mode
  continuation surfaces
- the lane still does not widen into full `newItems`, full `runContext`, or
  full `RunState`

Until then, `P22` should be described as:

- docs-backed
- issue-backed
- boundary-tight
- pre-proof on the live approval-interruption lane

## 11. Minimal runtime target

Use the smallest harness that can deterministically produce:

- one paused tool-approval interruption
- one serialized resumable state

Preferred first target:

- one top-level agent
- one simple function tool with `needsApproval`
- one non-realtime run
- no session
- no handoff
- no provider-mode mixing

Hard constraints:

- no transcript-history lane as the center of the sample
- no session integration in the first proof
- no `previousResponseId` chaining in the first proof
- no tracing or raw response export
- no full `newItems` capture as canonical contract

## 12. Concrete repo deliverable

If this lane is accepted, the first implementation PR should add:

- `examples/openai-agents-js-approval-interruption-evidence/README.md`
- `examples/openai-agents-js-approval-interruption-evidence/map_to_assay.py`
- `examples/openai-agents-js-approval-interruption-evidence/fixtures/valid.openai-agents-js.json`
- `examples/openai-agents-js-approval-interruption-evidence/fixtures/failure.openai-agents-js.json`
- `examples/openai-agents-js-approval-interruption-evidence/fixtures/malformed.openai-agents-js.json`
- `examples/openai-agents-js-approval-interruption-evidence/fixtures/valid.assay.ndjson`
- `examples/openai-agents-js-approval-interruption-evidence/fixtures/failure.assay.ndjson`

## 13. Valid / failure / malformed corpus

### 13.1 Valid

One artifact with:

- `pause_reason = tool_approval`
- one bounded `interruptions` list
- one bounded `resume_state_ref`

### 13.2 Failure

One weaker but still valid artifact with:

- `pause_reason = tool_approval`
- one bounded `interruptions` list
- one bounded `resume_state_ref`
- fewer optional reviewer aids present

This should still be a valid paused approval artifact.

It must not imply:

- a stable rejection result shape
- a stable approval outcome shape
- a native confidence or ranking model

v1 failure fixtures remain paused approval artifacts with fewer reviewer aids;
they do not claim stable approve/reject outcome semantics.

The lane is about pending interruption evidence first, not about resolved tool
call outcome truth.

### 13.3 Malformed

One malformed artifact that fails fast, for example:

- missing `interruptions`
- empty `interruptions`
- `pause_reason` not equal to `tool_approval`
- raw serialized `RunState` inlined as canonical evidence
- full `history` added to the artifact
- full `newItems` added to the artifact
- `lastResponseId` / session / history surfaces mixed in as if they defined the
  lane
- `resume_state_ref` given as a URL
- interruption items missing `tool_name` or `call_id_ref`

For v1, these cardinality and drift violations should be treated as malformed
rather than partially imported:

- `history`, `session`, and `lastResponseId` / `previousResponseId`-style
  continuation hints mixed together as co-equal lane-defining surfaces
- full transcript history
- full rich run-item arrays
- raw run-context dumps
- raw serialized `RunState` inline as canonical evidence

## 14. Outward strategy

The outward move should be issue-first, not show-and-tell-first.

Why:

- the repo uses issues, not GitHub Discussions, as the public seam-pressure
  channel
- the approval/resume boundary is already alive there
- the right outward move is likely one small sample-backed boundary question,
  not a broad feature pitch

The likely outward question should stay small and warm:

- we built a tiny external-consumer sample around one paused approval run
- we kept it on `interruptions` plus one resumable continuation anchor
- is that roughly the right minimal surface for an external evidence consumer
- or is there a thinner official seam you would rather point us at

Do not ask about:

- full session support
- transcript support
- provider-chaining support
- broad `RunState` export

## 15. Source anchors

Public sources used for this lane:

- OpenAI Agents JS README
- Results guide
- Human-in-the-loop guide
- Running agents guide
- Sessions guide
- GitHub issues `#1097` and `#1104`

## 16. Final judgment

`P22` is a strong candidate because it is:

- current
- small
- docs-backed
- issue-backed
- and much less likely to drift into transcript or persistence-truth inflation
  than a broader sessions/results lane

The core discipline is simple:

> Keep `P22` about one paused approval run, one bounded `interruptions` list,
> and one resumable continuation anchor, not about OpenAI Agents JS as a
> whole.
