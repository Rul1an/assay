# PLAN — P24 Phoenix Span Annotation Evaluation-Signal Evidence Interop (2026 Q2)

- **Date:** 2026-04-22
- **Owner:** Evidence / Product
- **Status:** Planning lane
- **Scope (current repo state):** Define one bounded Phoenix-adjacent lane
  centered on a single span annotation as an external evidence signal. This
  plan does **not** propose broad Phoenix support, trace export support,
  dataset or experiment support, prompt management support, or platform-level
  observability import.

## 1. Why `P24` should exist

`Arize-ai/phoenix` is now one of the clearest active adjacencies for Assay:

- it is public and actively maintained,
- it explicitly spans tracing, evaluation, datasets, and experiments,
- and it already names small feedback surfaces instead of only platform-wide
  dashboards.

That matters because Assay does **not** need Phoenix as a platform.

It needs the smallest honest external-consumer seam that:

- already exists in named public docs,
- is small enough to review without importing Phoenix platform truth,
- and is likely to stay meaningful even if the broader product surface keeps
  evolving.

The strongest first wedge is not a whole experiment or a whole trace.

It is:

- one bounded span annotation,
- on one named span target,
- with one small result bag,
- and only the minimal provenance needed to treat it as imported external
  evidence.

## 2. Why Phoenix and why now

Phoenix publicly positions itself as:

- tracing,
- evaluation,
- datasets,
- experiments,
- playground,
- and prompt management.

That is exactly why the first Assay wedge must be narrower than "Phoenix
interop."

Phoenix's own annotation documentation already exposes a smaller public shape:

- annotations can target spans, documents, traces, and sessions,
- every annotation has an entity id plus a `name`,
- and the result surface is explicitly bounded to one or more of `label`,
  `score`, and `explanation`, with optional `annotator_kind`, `identifier`,
  and `metadata`.

The Python client examples then make the seam concrete through:

- `client.spans.add_span_annotation(...)`
- `client.traces.add_trace_annotation(...)`
- related document/session annotation helpers

That is exactly the kind of public, named, smaller-than-platform surface Assay
should prefer.

## 3. Why this seam and not broader Phoenix surfaces

### 3.1 Why span annotations first

Phoenix offers four annotation targets:

- span
- document
- trace
- session

The smallest honest v1 wedge is the span annotation because it is:

- entity-scoped,
- feedback-shaped,
- already public in docs,
- and smaller than end-to-end trace or session judgments.

It also avoids importing document payloads and conversation/session semantics.

### 3.2 Why not full trace truth

Phoenix tracing is important and also far too broad as the first wedge.

A trace-first lane would immediately widen into:

- full span trees,
- prompt/completion payloads,
- token/cost/runtime context,
- and broader observability semantics.

That is the wrong first move.

### 3.3 Why not experiments first

Phoenix experiments are useful, but the first public experiment surface already
comes wrapped in:

- datasets,
- tasks,
- evaluators,
- and aggregate experiment results.

That is productively interesting and too broad for v1.

For Assay, experiments are a runner-up lane, not the first lane.

### 3.4 Why not datasets, playground, or prompt management

Those surfaces are all real and all broader than the first evidence seam.

They widen immediately into:

- content truth,
- version-management truth,
- replay semantics,
- or prompt-management semantics.

That is not the first external evidence wedge.

## 4. Hard positioning rule

This lane must stay smaller than the upstream ecosystem name.

Normative framing:

> `P24` v1 claims only one bounded Phoenix span annotation artifact as an
> imported external evaluation signal. It does not claim trace truth,
> experiment truth, evaluator truth, dataset truth, prompt truth, or Phoenix
> platform truth.

That means:

- Phoenix remains the source system, not Assay truth
- the annotation is imported as external evidence, not as platform judgment we
  inherit wholesale
- Assay stays smaller than Phoenix itself

Common anti-overclaim sentence:

> We are not asking Assay to inherit Phoenix trace, experiment, evaluator, or
> dashboard semantics as truth.

## 5. Proposed v1 seam

The first honest seam is:

- one `span_id`-scoped annotation
- one `name`
- one bounded result bag using only the fields Phoenix already names publicly
- one optional `annotator_kind`
- one optional `identifier`
- exactly one annotation object per artifact
- no broad trace or experiment wrapper

In Phoenix terms, the public shape we are leaning on is:

- entity id: `span_id`
- `name`
- one or more of `label`, `score`, `explanation`
- optional `annotator_kind`
- optional `identifier`
- optional `metadata`

For Assay, the imported artifact should stay smaller than that full permissive
shape when necessary.

That means:

- no batch annotation arrays
- no multi-span wrapper
- no "annotations export" bundle in one v1 artifact

## 6. Canonical artifact boundaries

The Assay-side reduced artifact for this lane should stay on:

Required:

- `schema`
- `framework`
- `surface`
- `entity_kind`
- `entity_id_ref`
- `annotation_name`
- `result`
- `timestamp`

Optional:

- `annotator_kind`
- `identifier`
- `metadata_ref`

Recommended fixed values for v1:

- `framework = "phoenix"`
- `surface = "span_annotation"`
- `entity_kind = "span"`

### 6.1 `entity_id_ref`

This is the bounded imported reference to the annotated span.

It must remain:

- opaque
- non-resolving
- entity-scoped

It must not become:

- a full span payload
- a trace tree
- a platform lookup URL

### 6.2 `annotation_name`

This should preserve the upstream annotation label/name directly when possible.

It must not be widened into:

- evaluator definitions,
- taxonomy truth,
- or a Phoenix-global ontology claim.

### 6.3 `result`

This should preserve only the bounded result fields that are actually present.

For v1, that means:

- `label` when present
- `score` when present
- `explanation` when present and bounded

For v1, at least one of `label` or `score` must be present.

`explanation` remains optional and bounded reviewer context, not a primary
carrier of the seam.

Whitespace-only or effectively empty explanations should be:

- omitted during reduction,
- or treated as malformed if the upstream payload is trying to rely on them as
  the only result content.

The mapper must not invent absent result fields.

### 6.4 `annotator_kind`

This is allowed only when naturally present upstream.

It is a useful bounded provenance field and should remain on the named Phoenix
surface:

- `HUMAN`
- `LLM`
- `CODE`

It is observed provenance only.

It must not be interpreted as:

- evaluator-quality truth
- runtime-quality truth
- or a guarantee about how reliable the annotation process was

It must not be widened into evaluator-runtime semantics.

### 6.5 `identifier`

This is allowed only as a bounded upsert/reference aid when naturally present.

Its presence may reflect upstream overwrite/upsert behavior.

It must not be treated as:

- a durable global identity guarantee,
- a cross-system reconciliation contract,
- or cross-run deduplication truth

Its absence must not be read as evidence that the annotation has no upstream
natural grouping or overwrite behavior.

### 6.6 `metadata_ref`

Phoenix annotations allow optional metadata.

For Assay v1, raw metadata should **not** be imported by default.

If metadata matters at all, keep it as:

- omitted,
- or a bounded `metadata_ref` / hash / reviewer aid

unless live proof shows a tiny stable subset is honestly necessary.

Raw annotation metadata inline is malformed for v1 unless a later discovery
pass proves that a tiny stable subset is honestly necessary.

## 7. What is explicitly out of scope

This lane is not for:

- multiple annotations in one artifact
- batch annotation wrappers
- multiple span targets in one artifact
- full trace export
- span trees
- experiment runs
- dataset example payloads
- prompt/completion bodies
- Phoenix dashboard URLs
- full annotation metadata objects
- session or document annotation truth

The first lane should stay on one span annotation artifact only.

Any v1 artifact containing:

- multiple annotations
- multiple span targets
- or a batch/export wrapper

should be malformed rather than partially imported.

## 8. Why this pick wins over the current shortlist

The current adjacent shortlist was:

- Phoenix
- LangWatch
- LangChain AgentEvals / agentevals-dev
- OpenHands SDK / benchmarks
- AutoGen
- browser-use
- smolagents

Phoenix wins the first `P24` slot because it combines:

- a large active upstream,
- explicit evaluation vocabulary,
- explicit annotation vocabulary,
- and a public surface that is already smaller than experiments or tracing.

That is better than:

- LangWatch for v1, because LangWatch still pulls more naturally toward
  trace/run wrappers than a single tiny feedback object
- AgentEvals for v1, because those repos are very clean but less likely to
  produce high-value upstream seam clarification than Phoenix
- OpenHands, AutoGen, browser-use, and smolagents, because those all create
  stronger drift pressure toward runtime/session/task truth

Runner-up order after Phoenix:

1. LangWatch evaluation item result lane
2. LangChain AgentEvals trajectory result lane
3. OpenHands benchmark case result lane

## 9. Discovery gate before any sample claim

`P24` should not freeze a sample from docs alone.

Required discovery order:

1. create one real Phoenix span annotation through the public client or API
2. retrieve that annotation again through the public get/list path when
   possible, so created and retrieved payloads can be compared
3. keep raw created payload and raw retrieved payload separate from the reduced
   Assay artifact
4. build a field presence/absence table
5. reduce to the smallest honest artifact
6. only then freeze fixtures and mapper behavior

The lane should fail closed if live proof shows the public payload is thinner
or differently shaped than the first frozen sample.

## 10. Outward strategy

The outward thread should stay in our normal shape:

- sample-first
- narrow question
- no integration pitch
- no platform-support framing

The core upstream question should be:

> If an external evidence consumer wants the smallest honest Phoenix feedback
> surface, is one reduced artifact derived from a span annotation roughly the
> right seam, or is there a thinner public annotation/result surface you would
> rather point them at?

That keeps the discussion on:

- one public object,
- one boundary question,
- and no broader Phoenix-import claim.

## 11. Implementation phases

### Phase A — Discovery

Deliverables:

- one real span annotation capture
- raw created payload freeze
- raw retrieved payload freeze when available
- field presence table
- observed-vs-derived notes

Acceptance:

- no field in the reduced artifact lacks live backing
- raw and reduced shapes are kept separate
- create-path and retrieve-path differences are explicitly noted when present

### Phase B — Sample lane

Deliverables:

- `examples/phoenix-span-annotation-evidence/`
- mapper
- valid / failure / malformed fixtures
- generated placeholder Assay outputs

Acceptance:

- valid maps
- failure maps
- malformed fails fast on metadata drift, batch drift, and multi-annotation
  drift

### Phase C — Outward check

Deliverables:

- one maintainer-facing issue/discussion
- one tiny sample link
- one sharply bounded seam question

Acceptance:

- no broad Phoenix support claim leaks in
- the lane remains annotation-first

## 12. Success criteria

This plan succeeds when:

- Assay has one honest Phoenix-adjacent sample lane
- the lane stays on one span annotation artifact
- the sample does not import trace or experiment truth
- the outward question asks only whether span annotation is the right minimal
  seam

## 13. Final judgment

`P24` should be a Phoenix annotation-first lane, not a Phoenix platform lane.

The right first wedge is:

- one bounded span annotation,
- one small result bag,
- one span-scoped reference,
- and nothing broader.
