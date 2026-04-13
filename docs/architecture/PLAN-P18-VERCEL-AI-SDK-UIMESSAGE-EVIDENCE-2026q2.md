# PLAN — P18 Vercel AI SDK UIMessage Evidence Interop (2026 Q2)

- **Date:** 2026-04-12
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next Vercel AI SDK interop lane after the
  current LiveKit and x402 wave. Include a small sample implementation, with
  no outward Discussion and no contract freeze yet.

## 1. Why this plan exists

After the current wave, the next adjacent lane should still pass the same
three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has a natural channel for one small sample-backed outreach move.

`vercel/ai` fits that pattern well enough to justify a formal plan:

- the repo is large, active, and current
- GitHub Discussions are enabled and heavily used
- the docs expose a clear message and stream surface around `UIMessage`,
  `streamText`, and the documented stream protocol
- the SDK already treats message shape and message metadata as a first-class
  interface

This is **not** a telemetry plan.

This is **not** a tracing plan.

This is **not** a tool-runtime correctness plan.

This is **not** a backend correctness plan.

This is a plan for a **bounded `UIMessage`-level evidence seam**.

## 2. Why Vercel AI SDK is a good `P18` candidate

Vercel AI SDK is strategically attractive because it sits in a different part
of the agent ecosystem than the current eval/result-heavy lanes.

Why:

- it gives Assay a message/stream contract lane rather than another evaluator
  or trace lane
- the SDK is widely used as an integration substrate for agent UIs and
  streaming backends
- the message surface is more portable and reviewable than jumping directly to
  telemetry or provider-specific transport internals

At the same time, the social norm is different from LlamaIndex.

The Discussions channel visibly leans toward:

- show and tell
- ecosystem integrations
- concrete examples

That means the best tactic is **show-and-tell-first**, not question-first.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets a bounded Vercel AI SDK message and stream surface at
> the `UIMessage` level, not telemetry, traces, tool-runtime correctness, or
> backend correctness truth.

That means:

- Vercel AI SDK is the upstream message/stream context, not the truth source
- `UIMessage` is a bounded message artifact, not a proof of task correctness
- Assay stays an external evidence consumer, not an authority on stream
  completeness, tool correctness, or UI correctness

Common anti-overclaim sentence:

> We are not asking Assay to inherit UI semantics, tool semantics, or
> streaming semantics as truth.

## 4. Why not telemetry-first

Vercel AI SDK also touches runtime internals, transports, and observability
adjacent concerns.

That would be the wrong first wedge.

Why:

- it would blur the lane into another generic platform integration
- it would skip the smaller and more explicit surface already documented in the
  SDK
- it would weaken the outward fit, because the repo's Discussions rhythm reads
  more like ecosystem sharing than observability boundary review

The cleaner first wedge is:

- one bounded `UIMessage`-level artifact
- one small message list
- one small metadata layer only where naturally present
- no traces
- no metrics
- no provider diagnostics

## 5. Why show-and-tell-first, not question-first

This lane should deliberately not use the same outward tactic as P17.

Why:

- the repo's Discussions surface is large and integration-heavy
- a pure seam-validation question is easier to ignore there
- a shipped, concrete sample reads more naturally in that environment

So the correct sequencing is:

1. build the sample
2. post a small show-and-tell Discussion with one link
3. optionally add one short boundary question at the end

## 6. Recommended v1 seam

Use **one frozen serialized artifact derived from a bounded subset of the
documented `UIMessage` and stream protocol surface** as the first
external-consumer seam.

The first artifact should stay message-first and use only a tiny
sample-level wrapper around the bounded `UIMessage` subset:

- one optional conversation or thread reference in the sample wrapper
- one bounded message list in the sample wrapper
- one optional small metadata layer
- no trace surface

Important framing rule:

> The sample uses a frozen wrapper artifact derived from the documented
> `UIMessage` surface, not a claim that Vercel AI SDK already guarantees one
> fixed wire export contract or one canonical conversation wrapper for
> external evidence consumers.

## 7. v1 artifact contract

### 7.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `timestamp`
- `messages`

### 7.2 Optional fields

The first sample may include:

- `thread_ref`
- `stream_protocol`
- `sdk_version_ref`

### 7.3 Important field boundaries

#### Wrapper fields

The v1 sample may use a tiny wrapper around the bounded `UIMessage` subset.

That means fields like:

- `thread_ref`
- top-level `messages`

belong to the sample wrapper, not to a claim that those exact fields are part
of one official upstream `UIMessage` export contract.

#### `messages`

This field is required because it is the smallest honest message-level seam.

It should remain:

- bounded
- ordered
- reviewable

It must not become:

- a trace dump
- a telemetry event stream
- a raw provider transcript dump

Each message record inside the sample wrapper should stay small and bounded:

- one `id`
- one `role`
- one bounded `parts` list
- optional bounded message metadata

#### Message parts

A v1 message should stay inside a small subset of the documented `UIMessage`
surface:

- `text` parts
- bounded `tool-*` parts
- optional short per-message metadata only where naturally present

Not allowed in v1:

- file parts
- source parts
- data parts
- reasoning parts
- giant tool argument blobs
- giant tool outputs
- raw provider request or response payloads
- screenshot or file payloads

#### Text parts

Text parts should remain:

- short
- reviewable
- content-first

They must not become:

- multi-message transcript dumps
- provider request bodies
- giant streaming deltas serialized back into one large block

#### Tool parts

Tool parts may carry bounded tool context only:

- `toolCallId`
- `state`
- small `input`
- small `output` or short `errorText`

They must not become:

- verbose tool payload captures
- debug transcripts
- backend execution truth

#### Metadata

Message metadata is optional and should stay bounded:

- timestamps
- short model labels
- small token counts

No v1 support for:

- verbose token accounting
- trace references
- debug payloads

## 8. Assay-side meaning

The sample may only claim bounded message and stream observation.

Assay must not treat as truth:

- backend correctness
- tool correctness
- stream completeness
- UI correctness

## 9. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/vercel-ai-uimessage-evidence/README.md`
- `examples/vercel-ai-uimessage-evidence/map_to_assay.py`
- `examples/vercel-ai-uimessage-evidence/fixtures/valid.vercel-ai.json`
- `examples/vercel-ai-uimessage-evidence/fixtures/failure.vercel-ai.json`
- `examples/vercel-ai-uimessage-evidence/fixtures/malformed.vercel-ai.json`
- `examples/vercel-ai-uimessage-evidence/fixtures/valid.assay.ndjson`
- `examples/vercel-ai-uimessage-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures should remain message-first
- v1 fixtures must not include traces or metrics
- v1 should keep metadata secondary and bounded
- v1 may use a tiny sample wrapper, but must not imply that the wrapper itself
  is an official upstream contract

## 10. Generator policy

The implementation should prefer a small local generator **only if** it stays
deterministic and does not require provider credentials.

Preferred:

- docs-backed frozen artifacts
- or a tiny local stream simulation that emits a bounded `UIMessage`-like
  shape

Avoid:

- provider credentials
- real production backends
- hidden observability setup

## 11. Outward strategy

Do not open a Vercel AI SDK Discussion until the sample is on `main`.

After that:

- one GitHub Discussion
- show-and-tell first
- one link
- one compact explanation
- optional short closing question
- no telemetry pitch
- no trace pitch

Suggested outward shape:

> Show and tell: small external consumer for AI SDK `UIMessage` streams

Optional closing question:

> If there is a thinner official result surface you would rather point
> external consumers at, happy to tighten the sample.

## 12. Non-goals

This plan does not:

- define a telemetry adapter
- define a tracing adapter
- define backend correctness as Assay truth
- define tool correctness as Assay truth

## References

- [Vercel AI repo](https://github.com/vercel/ai)
- [Vercel AI discussions](https://github.com/vercel/ai/discussions)
- [AI SDK introduction](https://ai-sdk.dev/docs/introduction)
- [AI SDK `UIMessage` reference](https://ai-sdk.dev/docs/reference/ai-sdk-core/ui-message)
- [AI SDK `streamText` reference](https://ai-sdk.dev/docs/reference/ai-sdk-core/stream-text)
- [AI SDK stream protocol](https://ai-sdk.dev/docs/ai-sdk-ui/stream-protocol)
- [AI SDK message metadata](https://ai-sdk.dev/docs/ai-sdk-ui/message-metadata)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
