# PLAN — P20 AG-UI Compacted Message Snapshot Artifact Evidence Interop (2026 Q2)

- **Date:** 2026-04-14
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this plan and sample):** Define the AG-UI compacted-message-history
  lane, include one small frozen sample implementation, and keep the lane
  explicitly pre-proof on a real snapshot-emitting integration path. No outward
  Discussion and no contract freeze yet.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has at least one natural maintainer or community channel for one
   small sample-backed boundary question.

`ag-ui-protocol/ag-ui` fits those tests unusually well:

- the repo is large, active, and was still pushed on 2026-04-14
- GitHub Discussions are enabled
- the public docs define both the event model and the
  serialization/compaction model
- the repo is actively discussing snapshot scope, compaction, and sequence
  validation in the open

This is **not** a plan for AG-UI event-stream support in general.

This is **not** a plan for AG-UI serialization support in general.

This is **not** a plan for frontend state synchronization, replay completeness,
branching correctness, or UI activity semantics.

This is a plan for a **bounded compacted message snapshot artifact lane**.

## 2. Why AG-UI is a good `P20` candidate

AG-UI is one of the strongest bleeding-edge adjacent protocol lanes available
right now.

Why:

- it is a real protocol layer, not just one framework's private runtime
- the docs make the candidate seam explicit through `RUN_STARTED`,
  `MESSAGES_SNAPSHOT`, `RUN_FINISHED`, `RUN_ERROR`, and event compaction
- the repo already has a broad supported integration set, with additional lanes
  still marked in progress
- open issues show that compacted snapshots are both important and still
  semantically alive upstream

That makes AG-UI stronger than another framework-specific message lane and
cleaner than jumping directly into a broad live-stream or frontend-state lane.

## 3. Hard positioning rule

This lane must stay smaller than the upstream ecosystem name.

Normative framing:

> `P20` v1 claims only portable compacted message-history evidence for a
> bounded run envelope; it does not claim protocol-complete AG-UI stream
> reconstruction.

That means:

- AG-UI is the upstream protocol context, not Assay truth
- a compacted snapshot artifact is an observed portable envelope, not the truth
  of AG-UI conversation state
- Assay stays an external evidence consumer, not an authority on branching,
  replay, or frontend state semantics

Common anti-overclaim sentence:

> We are not asking Assay to inherit AG-UI stream fidelity, state sync,
> branching correctness, or replay completeness as truth.

## 4. Why not AG-UI serialization support in general

The AG-UI serialization docs are richer than the first seam we want.

They explicitly cover:

- history restore
- reconnect / attach to running agents
- branching and `parentRunId`
- state compaction through `STATE_SNAPSHOT`
- input normalization in `RunStarted.input`

That is broader than the first honest Assay wedge.

If we call this "AG-UI serialization interop" without qualification, the lane
will silently overread the upstream surface and invite product drift.

The correct first wedge is smaller:

- one serialized artifact
- one bounded run envelope
- one compacted `MESSAGES_SNAPSHOT`
- one run close event

Everything else stays outside v1 unless later evidence forces it in.

Important caveat:

> The current JavaScript `compactEvents` utility reduces verbose text and tool
> call sequences, but it does **not** generate `MESSAGES_SNAPSHOT`.

So `P20` must not pretend that one official compaction helper already yields
the full v1 artifact. If we later freeze a sample, it must be derived from a
real serialized or integration-emitted run envelope that already includes a
bounded message snapshot seam, or from one clearly named projection step on top
of that envelope.

## 5. Why not the full event stream

The raw AG-UI stream is valuable, but it is the wrong first lane.

Why:

- it widens immediately into transport, timing, and ordering concerns
- it increases pressure to claim replay fidelity
- it invites frontend activity and state sync semantics into the contract
- it makes one small external evidence consumer look like a protocol-complete
  import path

The smaller first seam is the compacted portable artifact, not the unbounded
live stream.

## 6. Why `MESSAGES_SNAPSHOT` is still the right first wedge

`MESSAGES_SNAPSHOT` is not "safe because it is final."

Upstream is still discussing snapshot scope and compaction semantics, and there
are open bugs where a missing or late snapshot breaks recovery.

That is exactly why this seam is worth testing.

The v1 posture is therefore:

- use a bounded portable snapshot seam because it is the smallest honest
  message-history artifact the protocol already names
- do **not** treat that snapshot as the truth of AG-UI conversation state
- do **not** claim that one snapshot preserves every step-, branch-, or
  transport-level semantic

This lane is about portable compacted message-history evidence, not full
protocol truth.

## 7. Recommended v1 seam

Use **one frozen artifact derived from a serialized AG-UI run envelope that
includes one bounded compacted message snapshot seam** as the first
external-consumer seam.

The first artifact should be built around:

- one `RUN_STARTED`
- exactly one bounded `MESSAGES_SNAPSHOT`
- exactly one `RUN_FINISHED` or `RUN_ERROR`

For v1, any frozen artifact containing additional AG-UI event families beyond
`RUN_STARTED`, one `MESSAGES_SNAPSHOT`, and one terminal event should be
treated as malformed rather than partially imported.

Important framing rule:

> The sample should use a frozen artifact derived from one serialized AG-UI run
> envelope with a bounded compacted message snapshot seam, not a claim that
> Assay models the AG-UI serialization spec as a whole.

In practice, this means the first sample should be framed as:

> a bounded compacted message snapshot artifact derived from one AG-UI run
> envelope,

not:

> AG-UI serialization support.

## 8. v1 artifact contract

### 8.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `thread_id_ref`
- `run_id_ref`
- `started_at`
- `messages`
- `terminal_event`

### 8.2 Optional fields

The first sample may include:

- `finished_at`
- `parent_run_id_ref`
- `error_code`
- `error_message`

### 8.3 Important field boundaries

#### Wrapper self-description

`schema`, `framework`, and `surface` are useful in the sample, but they are
wrapper self-description first, not the core truth of the evidence seam.

In v1:

- `schema` identifies the frozen sample artifact shape
- `framework` names the upstream protocol context
- `surface` names the bounded seam hypothesis

They must not be overread as proof that upstream itself ships one canonical
export wrapper with those same classification labels.

#### Envelope identity

`thread_id_ref` and `run_id_ref` are allowed because the run envelope is not
reviewable without bounded identity anchors.

They must remain:

- opaque identifiers
- portable
- non-resolving

They must not become:

- URLs
- dashboard links
- transport-specific handles

#### `messages`

This field is required because the lane is fundamentally a compacted
message-history seam.

It must remain:

- bounded
- reviewable
- ordered
- portable

It must not become:

- a full raw event log
- a replay transcript with chunk-level fidelity claims
- a frontend state dump

Each v1 message should stay on the narrowest portable subset:

- `id`
- `role`
- `content`
- optional `name`

Nothing else should enter the first sample unless one real implementation
forces it.

For v1, `messages` accepts only a plain-text, role-labeled, ordered normalized
subset. Richer AG-UI message content remains out of scope even if upstream
supports it.

#### Message content

The first sample should stay on the smallest honest message subset.

Allow in v1:

- bounded message identifiers
- bounded message role
- bounded text content

Not allowed in v1:

- encrypted reasoning payloads
- multimodal blobs
- frontend activity events
- arbitrary vendor metadata bags

This is a bounded projection choice, not a claim that richer upstream AG-UI
messages are invalid. It only means they remain out of this first Assay lane.

#### Envelope timestamps

`started_at` and `finished_at` are allowed because the frozen artifact needs a
small run envelope.

They must be read as observed timestamp labels for the bounded artifact only.

They must not be overread as:

- transport-order truth
- replay-completeness proof
- end-to-end protocol timing guarantees
- proof that no earlier or later AG-UI events existed outside the artifact

In other words, these timestamps label the compacted envelope; they do not
certify full AG-UI ordering or completeness semantics beyond it.

#### `terminal_event`

This field is required because the artifact must close the bounded run
envelope.

In v1 it should stay tiny:

- `RUN_FINISHED`
- `RUN_ERROR`

This is an observed run close label, not a claim that Assay independently
validated protocol completeness.

#### Error details

If the artifact closes with `RUN_ERROR`, keep failure context small:

- one short error message
- optional short error code

Do not include:

- stack traces
- raw transport failures
- replay instructions

#### `parent_run_id_ref`

This field is optional because AG-UI serialization includes lineage, but
branching correctness is outside the first lane.

If present, it must stay:

- opaque
- bounded
- informational only

It must not cause the v1 sample to claim time-travel or branch-replay truth.

Absence of `parent_run_id_ref` must not be interpreted as proof that no branch
lineage exists. In v1, it is optional because reviewability does not require
lineage to be present in every frozen artifact.

## 9. Explicit non-goals

`P20` v1 must not absorb the rest of AG-UI's serialization model.

Not part of v1:

- `STATE_SNAPSHOT`
- `STATE_DELTA`
- `RAW`
- `CUSTOM`
- tool call streaming fidelity
- step-level completeness
- frontend activity semantics
- replay completeness claims
- branch correctness claims
- transport semantics
- `RunStarted.input` normalization as a truth surface

If any of those enter the sample, the lane has drifted.

## 10. Why AG-UI is socially ripe

The repo is not just active; it is already showing seam pressure in the open.

Signals that matter:

- open issue on event compaction in the Python SDK
- open issue on missing `MESSAGES_SNAPSHOT` breaking later turns
- open issue on scoped message snapshots
- open issue on sequence validation

That means a small sample-backed question about the narrow portable snapshot
seam is likely to read as adjacent to real protocol work, not as a random
consumer request.

## 11. Why not the strongest alternatives

### Cloudflare Agents

Cloudflare Agents is a strong adjacent repo, but the social and technical shape
is worse for this lane right now.

Why:

- the most tempting seam is the Session API, which is explicitly experimental
- the session surface is broader and more product-heavy than the first Assay
  wedge should be
- the repo README says they are not accepting external PRs right now

That makes it less ripe for our usual sample-first loop.

### assistant-ui

assistant-ui is healthy and relevant, but it overlaps too much with the
existing message-first `P18` territory.

It feels more like a later UI runtime lane than the best next fresh protocol
lane.

### Letta Agent File

Letta's `.af` format is intellectually interesting, but it is too rich too
fast for the first wedge.

It packages prompts, memory, tools, and model settings in one portable file.
That is exactly the kind of surface that would make Assay overclaim if we are
not ruthless.

## 12. Sample shape recommendation

The first sample should stay planar and boring.

Recommended first artifact shape:

- one wrapper artifact around one bounded run envelope
- one bounded identity pair: `thread_id_ref`, `run_id_ref`
- one `started_at`
- one compacted message list under `messages`
- one terminal label under `terminal_event`
- optional `finished_at`
- optional bounded lineage pointer
- optional short error context only when `terminal_event=RUN_ERROR`

The frozen artifact must not smuggle in additional AG-UI event families beside
that bounded envelope. If it does, the v1 posture is malformed, not
"best-effort partial import."

First sample path:

- `examples/ag-ui-compacted-message-snapshot-evidence/`

Suggested frozen top-level shape:

```json
{
  "schema": "ag-ui.compacted-message-snapshot.export.v1",
  "framework": "ag_ui",
  "surface": "compacted_message_snapshot_artifact",
  "thread_id_ref": "thread_123",
  "run_id_ref": "run_456",
  "started_at": "2026-04-14T19:00:00Z",
  "messages": [
    {
      "id": "m1",
      "role": "user",
      "content": "Hello"
    },
    {
      "id": "m2",
      "role": "assistant",
      "content": "Hi there"
    }
  ],
  "terminal_event": "RUN_FINISHED",
  "finished_at": "2026-04-14T19:00:02Z"
}
```

This is intentionally smaller than:

- the AG-UI event stream
- the AG-UI serialization model
- AG-UI state management
- AG-UI restore / attach / replay semantics

## 13. Outward posture

When this lane eventually goes outward, the message should be small and
field-boundary-first.

The right question is not:

> "Is this AG-UI serialization support?"

The right question is:

> "If an external evidence consumer wants the smallest honest portable
> compacted message-history artifact for one bounded run envelope, is this
> roughly the right place to start, or is there a thinner official seam?"

That keeps the posture warm, specific, and non-theatrical.

## 14. Discovery gate before any stronger claim

Before this sample is treated as proven against live upstream behavior, do one
small discovery pass against the current AG-UI docs and one real integration.

Discovery must confirm:

- whether one compacted `MESSAGES_SNAPSHOT` is really the smallest portable
  artifact we can freeze without importing state sync
- whether any required v1 identity anchor is still too rich
- whether `parentRunId` is common enough to warrant fixture coverage but still
  optional in v1
- whether one supported integration can produce the bounded artifact without
  dragging in activity or state semantics

### 14.1 First implementation target

The first real implementation target should be the **official ADK middleware**
path, specifically the documented `emit_messages_snapshot=True` option.

Why this is the right first proof target:

- it is already listed by AG-UI as a supported integration
- upstream docs explicitly document end-of-run `MESSAGES_SNAPSHOT` emission
- it gives us one real snapshot-emitting path without forcing a full stream
  reconstruction project
- it is narrower than using the experimental `/agents/state` endpoint, which
  also returns state and would widen the lane too early

Normative rule:

> The first `P20` proof target is the documented end-of-run snapshot-emission
> path, not the broader AG-UI state retrieval path.

### 14.2 Discovery steps

The first discovery pass should stay brutally small:

1. run one supported AG-UI implementation that explicitly emits
   `MESSAGES_SNAPSHOT`
2. capture one bounded run envelope from that implementation
3. preserve the raw emitted event sequence
4. derive the smallest possible frozen artifact that still preserves portable
   message-history meaning
5. compare that artifact against the planned v1 contract before freezing any
   stronger claims

### 14.3 Discovery done definition

Discovery is done only when all of the following are true:

- one reproducible integration-emitted `MESSAGES_SNAPSHOT` capture exists
- the raw captured run envelope is preserved
- we know whether the implementation emits exactly one usable snapshot per
  bounded run
- we know whether `parentRunId` is absent, incidental, or common in the tested
  path
- we can show that the frozen artifact is smaller than the upstream envelope
  without pretending to preserve full protocol semantics

## 15. Exit criterion for `P20`

`P20` is not ready to close on plan text or a docs-backed sample alone.

This lane is only ready to close when both conditions hold:

- one bounded sample lane exists in-repo
- one real snapshot-emitting integration capture has been compared against that
  sample contract

Until then, `P20` remains an implemented but still pre-proof lane, not a claim
that Assay supports AG-UI serialization broadly.
