# PLAN — P19 Mem0 Add Memories Result Evidence Interop (2026 Q2)

- **Date:** 2026-04-13
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next Mem0 interop lane after the current
  LlamaIndex, Vercel AI SDK, LiveKit, x402, Mastra, Langfuse, Browser Use,
  and Visa TAP wave. Include a small sample implementation, with no outward
  Discussion and no contract freeze yet.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has at least one natural maintainer or community channel for one
   small sample-backed boundary question.

`mem0ai/mem0` fits that pattern well enough to justify a formal plan:

- the repo is large, active, and still moving quickly
- GitHub Discussions are enabled and visibly used, including a `Q&A` category
- the public Mem0 API surface exposes a small operation-result seam around
  `Add Memories`
- the result seam is much narrower than broader memory search, graph, or
  retrieval semantics

This is **not** a memory-search plan.

This is **not** a retrieval-truth plan.

This is **not** a graph export plan.

This is **not** a conversation-state truth plan.

This is a plan for a **bounded `Add Memories` result seam**.

## 2. Why Mem0 is a good `P19` candidate

Mem0 is the strongest next adjacent lane after the current message and
evaluation wave.

Why:

- it opens a new memory-operation class without collapsing back into another
  trace or evaluator lane
- the `Add Memories` path yields a small structured result that already looks
  like external evidence
- the repo has a natural `Q&A` channel for one small seam-check question
- the seam is operational and bounded instead of forcing us into broader memory
  or retrieval semantics

That makes Mem0 cleaner than jumping directly to memory search, graph memory,
or broader "memory truth" posture.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest Mem0 memory-mutation result
> surface exposed through `Add Memories`, not retrieval truth, graph truth,
> user-profile truth, or application-state truth.

That means:

- Mem0 is the upstream memory-operation context, not the truth source
- `Add Memories` results are observed mutation outputs, not Assay truth
  judgments
- Assay stays an external evidence consumer, not an authority on whether a
  memory should exist or whether a downstream memory model is correct

Common anti-overclaim sentence:

> We are not asking Assay to inherit Mem0 memory semantics, retrieval
> semantics, or user-profile semantics as truth.

## 4. Why not search-first

Mem0 also exposes broader search and retrieval surfaces.

That would be the wrong first wedge.

Why:

- it would immediately drag the lane into relevance, ranking, and retrieval
  semantics
- it would make overclaiming much easier
- it would blur the difference between an observed mutation result and
  downstream memory truth
- it would skip the smaller operation-result seam already documented in the
  API

The cleaner first wedge is:

- one frozen serialized artifact derived from `Add Memories`
- one bounded `results[]` list
- one bounded event label per result
- one bounded memory text field only
- no search results
- no embeddings
- no graph structure
- no prompt or transcript payloads

## 5. Why mutation-result-first, not memory-truth-first

The public Mem0 surface makes it tempting to talk about "memory" as if the
result object were already the application's truth.

That would be the wrong first posture.

Why:

- it would overstate what the sample actually observes
- it would make user-profile or preference truth too easy to imply
- it would widen the lane from observed mutation output to durable semantic
  correctness

The correct first seam is smaller:

- one bounded mutation-result artifact
- one operation label
- one result event
- one bounded memory string

That keeps `P19` aligned with Assay's evidence-consumer posture rather than a
memory-platform integration pitch.

## 6. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented `Add
Memories` result path** as the first external-consumer seam.

The first artifact should stay mutation-result-first and use only the smallest
official-looking result wrapper needed for the sample:

- one bounded top-level `results` list
- one bounded `event` label per result
- one bounded memory text field inside `data`
- one required `id` per result

Important framing rule:

> The sample uses a frozen artifact derived from the documented `Add
> Memories` result surface, not a claim that Mem0 already guarantees one fixed
> universal wire-export contract for external evidence consumers.

## 7. v1 artifact contract

### 7.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `timestamp`
- `operation`
- `output_format`
- `results`

### 7.2 Optional fields

The first sample may include:

- `user_ref`
- `agent_ref`
- `run_ref`
- `version_ref`

### 7.3 Important field boundaries

#### Wrapper fields

The v1 sample may use a tiny wrapper around the bounded `Add Memories`
results.

That means fields like:

- `timestamp`
- `operation`
- `output_format`
- optional refs such as `user_ref`

belong to the sample wrapper, not to a claim that those exact fields are part
of one fixed universal upstream export contract.

#### `results`

This field is required because it is the smallest honest mutation-result seam.

It must remain:

- bounded
- ordered
- reviewable

It must not become:

- a memory graph dump
- a retrieval result set
- a profile export

Each result record should stay small and bounded:

- one `id`
- one `event`
- one bounded `data` object

#### `event`

This field is required because the result surface is only meaningful if we
keep the mutation classification.

In v1, keep it small:

- `ADD`
- `UPDATE`
- `DELETE`

It must remain an observed upstream mutation label, not a claim that Assay
independently validated the memory semantics.

#### `data`

This field is required in v1, but it must stay very small.

Keep it to:

- one bounded `memory` string

Not allowed in v1:

- embeddings
- graph edges
- verbose metadata maps
- conversation transcripts
- prompt bodies
- large source documents

#### Memory text

`data.memory` must remain:

- short
- reviewable
- bounded

It must not become:

- a transcript chunk
- a prompt dump
- a rich profile object

#### Optional refs

The optional reference fields must stay bounded:

- opaque ids
- short labels
- no PII-first identifiers

Not allowed in v1:

- email addresses
- phone numbers
- raw user ids that encode sensitive profile data

## 8. Assay-side meaning

The sample may only claim bounded memory-mutation observation.

Assay must not treat as truth:

- user preference truth
- durable profile truth
- retrieval correctness
- ranking correctness
- graph correctness
- application correctness beyond the observed mutation artifact

## 9. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/mem0-add-memories-evidence/README.md`
- `examples/mem0-add-memories-evidence/map_to_assay.py`
- `examples/mem0-add-memories-evidence/fixtures/valid.mem0.json`
- `examples/mem0-add-memories-evidence/fixtures/failure.mem0.json`
- `examples/mem0-add-memories-evidence/fixtures/malformed.mem0.json`
- `examples/mem0-add-memories-evidence/fixtures/valid.assay.ndjson`
- `examples/mem0-add-memories-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures should remain mutation-result-first
- v1 fixtures must not include search or graph surfaces
- v1 may omit optional refs entirely if the shape stays cleaner that way

## 10. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 10.1 Preferred path

Preferred:

- docs-backed frozen artifacts
- a tiny local generator only if it produces deterministic bounded result
  shapes without provider credentials

### 10.2 Hard fallback rule

If a real generator would require:

- remote providers
- credentials
- live vector stores
- a larger memory harness

then the sample should fall back to a **docs-backed frozen artifact shape**.

## 11. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 11.1 Valid

One valid mutation result artifact with:

- one `ADD` event
- one bounded `memory` string
- no optional refs required

### 11.2 Failure

One valid bounded mutation result artifact with:

- one non-`ADD` event such as `UPDATE`
- one bounded `memory` string
- no infrastructure failure semantics

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded mutation
artifact, not an infrastructure failure.

### 11.3 Malformed

One malformed artifact that fails fast, for example:

- unsupported `event`
- missing `results`
- `data.memory` not a short string

## 12. Outward strategy

Do not open a Mem0 Discussion until the sample is on `main`.

After that:

- one GitHub Discussion in `Q&A`
- one link
- one boundary question
- no broad product pitch
- no retrieval-truth pitch

Suggested outward question:

> If an external evidence consumer wants the smallest honest Mem0 surface, is
> a bounded artifact derived from the `Add Memories` result path roughly the
> right place to start, or is there a thinner official mutation-result surface
> you would rather point them at?

## 13. Non-goals

This plan does not:

- define a Mem0 search adapter
- define a graph export adapter
- define retrieval correctness as Assay truth
- define user-profile semantics as Assay truth

## References

- [Mem0 repo](https://github.com/mem0ai/mem0)
- [Mem0 discussions](https://github.com/mem0ai/mem0/discussions)
- [Mem0 Add Memories API](https://docs.mem0.ai/api-reference/memory/add-memories)
- [Mem0 Search Memory docs](https://docs.mem0.ai/core-concepts/memory-operations/search)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
