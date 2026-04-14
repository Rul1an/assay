# PLAN — P14b Mastra ScoreEvent / ExportedScore Evidence Interop (2026 Q2)

- **Date:** 2026-04-13
- **Owner:** Evidence / Product
- **Status:** Docs-backed sample implementation; runtime capture pending
- **Scope (current repo state):** Recut the Mastra lane after maintainer
  feedback on `mastra-ai/mastra#15206`, and carry that recut into a bounded
  sample implementation. This slice still does not freeze a new upstream
  contract or reopen outward follow-up yet.

## 1. Why this recut exists

`P14` started from a reasonable scorer / experiment-item seam hypothesis, but
the maintainer replies tightened the target twice.

On 2026-04-13, a Mastra maintainer first replied that:

- scorer definitions are not the right external-consumer surface
- experiment-item results are not likely to be where scored output lives
  going forward
- score results are expected to live in the observability scores table

Later that same day, the same maintainer made the seam more concrete:

- the right narrow integration point is the `ObservabilityExporter` path
- exporters receive typed `ScoreEvent` signals
- the bounded payload is `ExportedScore`

That changes the lane.

This recut exists so Assay does not cling to the first seam hypothesis after
upstream has already pointed to a better one.

This is therefore a **maintainer-driven recut**, not a brand-new lane.

## 2. What changed from P14

`P14` was framed around:

- scorer output
- experiment-item context
- dataset version context

`P14b` now pivots to:

- one bounded typed score event
- exporter-first score-result observation
- `ExportedScore`-derived shape

What drops from the center of the lane:

- scorer definitions as the main seam
- experiment-item wrappers as the main seam
- “score table row” as the main implementation story
- dataset version refs as required fields

What stays true:

- no tracing-first posture
- no Studio/dashboard truth posture
- no broad observability export pitch
- no overclaim that upstream semantics become Assay truth

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest Mastra score-result surface derived
> from the current `ObservabilityExporter` / `ScoreEvent` path, not scorer
> definitions, experiment summaries, traces, dashboards, or runtime
> correctness truth.

That means:

- Mastra is the upstream reliability context, not the truth source
- a typed score event is an observed upstream artifact, not Assay truth
- Assay stays an external evidence consumer, not a scorer, dashboard, or trace
  authority

Common anti-overclaim sentence:

> We are not asking Assay to inherit Mastra scoring semantics, observability
> semantics, or runtime semantics as truth.

## 3.1 Terminology alignment

Mastra's public exporter surface is often described in terms of the exporter
score hook and payload fields such as `traceId`, `spanId`, `score`, `reason`,
`scorerName`, and `metadata`.

The maintainer response on `#15206` described the same seam more explicitly as
typed `ScoreEvent` signals carrying `ExportedScore`.

`P14b` should use both names carefully and avoid pretending they are two
different seams.

For the sample contract:

- `trace_id_ref` maps to `traceId`
- `span_id_ref` maps to `spanId`
- `score` maps to `score`
- `reason` maps to `reason`
- `scorer_name` maps to `scorerName`
- `metadata_ref` is a bounded reference standing in for `metadata`
- `target_ref` is a sample-level bounded anchor derived from exporter payload
  anchors, not a claim that Mastra publishes one official `targetRef` field

## 4. Why exporter-first score events are the right recut

The maintainer signal now points to a more concrete seam than the earlier
"score storage path" framing.

The stronger seam is:

- `ObservabilityExporter`
- `ScoreEvent`
- `ExportedScore`

Why this is stronger than the original seam:

- it is thinner than scorer-definition + experiment-item composition
- it follows the current product direction instead of an older modeling guess
- it is a typed integration point, not just a guessed storage shape
- it keeps the lane score-first without dragging in full tracing or dashboard
  payloads
- it better matches what an external evidence consumer actually needs: one
  bounded reliability signal with provenance

This is still not a trace lane.

It is also still not a dashboard lane.

It is the narrow middle path:

- one typed score event
- one scorer identity
- one bounded target anchor set
- one timestamp
- optional bounded reason only if naturally present

## 5. Why not observability-first in the broad sense

The maintainer answer points us toward the observability exporter path, but
that must not be misread as license for a broad observability import.

That would be the wrong response.

Why:

- it would immediately widen the lane back into logs, traces, metrics, or
  Studio semantics
- it would undo the bounded-seam discipline that made the original Mastra
  sample worthwhile
- it would turn one precise redirect into a platform-wide export hypothesis

So the recut rule is:

- **`ScoreEvent` yes**
- **trace tree no**
- **dashboard summary no**
- **general observability sink no**

## 6. Recommended v1 seam

Use **one frozen serialized artifact derived from Mastra's score exporter
path** as the first external-consumer seam.

The seam should stay typed and bounded:

- one scorer identity
- one numeric score value
- one bounded target anchor
- one timestamp
- optional target entity type
- optional short reason
- optional trace/span anchors only if naturally present

Important framing rule:

> The sample uses a frozen artifact derived from the current
> `ObservabilityExporter` score path, not a claim that Mastra already
> guarantees one fixed external export contract for all observability
> consumers.

## 6.1 Current upstream code reality

The maintainer guidance points to `ObservabilityExporter` + `ScoreEvent` +
`ExportedScore`, but the current upstream code shows one important asymmetry:

- the score types define `ScoreEvent` and `ExportedScore`
- `ObservabilityEvents` exposes `onScoreEvent`
- the scorer hook currently calls `exporter.addScoreToTrace(...)`
- that current callback shape is narrower than `ExportedScore`

At the current upstream revision reviewed for this recut, the active
`addScoreToTrace(...)` callback carries:

- `traceId`
- `spanId`
- `score`
- `reason`
- `scorerName`
- `metadata`

Notably, that path does **not** obviously guarantee:

- `scorerId`
- `targetEntityType`
- `scoreSource`
- `correlationContext`

So the current P14b sample must stay honest about two adjacent truths:

- the richer typed seam exists in upstream types and maintainer guidance
- the currently wired exporter callback visible in code is thinner

## 7. v1 artifact contract

### 7.1 Required fields

The first recut sample should require:

- `schema`
- `framework`
- `surface`
- `timestamp`
- `score`
- `target_ref`

And it should require **at least one scorer identity field**:

- `scorer_id`, or
- `scorer_name`

### 7.2 Optional fields

The first recut sample may include:

- `scorer_id`
- `scorer_name`
- `target_entity_type`
- `reason`
- `trace_id_ref`
- `span_id_ref`
- `scorer_version`
- `score_source`
- `metadata_ref`

### 7.3 Important field boundaries

#### `scorer_id` / `scorer_name`

At least one of these fields is required because the score is not meaningful
without a bounded identity for the scorer that produced it.

Why this is not stricter:

- the richer typed `ExportedScore` shape includes `scorerId`
- the currently wired `addScoreToTrace(...)` callback in upstream code only
  obviously guarantees `scorerName`

So the sample should require one bounded scorer identity, not pretend both are
always present on the live exporter path.

In v1 they must stay small:

- short scorer identifier
- short scorer label

Not allowed:

- full scorer definition
- full scorer pipeline config
- model prompt or judge prompt

#### `score`

This field is required and should remain scalar and numeric in v1:

- one numeric score

Not allowed in v1:

- full score breakdown matrix
- aggregate experiment rollups
- score histograms

#### `target_ref`

This field is required because an external evidence consumer needs one bounded
anchor for what was scored, not just a type label.

It must remain:

- opaque
- short
- resolver-free

Allowed:

- short trace-like id
- short span-like id
- short entity id when it is the natural exporter anchor

Not allowed in v1:

- request/response bodies
- prompts
- output payloads
- URLs into dashboards or traces

#### `target_entity_type`

This field is optional in v1.

Why:

- the richer typed `ExportedScore` shape includes `targetEntityType`
- the currently wired `addScoreToTrace(...)` callback does not obviously
  guarantee it

So this field is still useful when present, but it should not be a hard
required field until a real capture proves it is consistently emitted on the
path we are actually targeting.

It must remain:

- short
- categorical
- reviewable

Not allowed in v1:

- full output body
- full request/response pair
- prompt text
- application-side wrapper semantics disguised as upstream truth

#### `reason`

This field is optional and should stay short and bounded.

Preferred:

- short explanation
- short bounded reason text

Not allowed in v1:

- long judge explanation
- free-form evaluator transcript
- trace-derived payload
- multiline text
- prompt or stack-trace dumps

#### `trace_id_ref` / `span_id_ref`

These fields are optional anchors only.

They may be present because the upstream `ExportedScore` can carry them, but
they must not change the lane into a trace lane.

Allowed:

- short opaque trace id
- short opaque span id

Not allowed:

- pulling full trace payloads
- resolving spans into dashboards or event trees inside the sample
- URLs
- resolver paths

## 8. Assay-side meaning

The recut sample may only claim bounded typed score-event observation.

Assay must not treat as truth:

- model correctness
- runtime correctness
- trace correctness
- dashboard correctness
- experiment summary truth

The score event is one bounded external signal, not a framework truth import.

## 9. Discovery gate before implementation

This recut should not ship another purely speculative sample.

Before the next sample PR, do one bounded discovery pass:

1. build a tiny real Mastra app with one scorer enabled
2. register a custom `ObservabilityExporter`
3. capture one real `ScoreEvent`
4. inspect the resulting `ExportedScore` shape
5. inspect both the richer typed score-event path and the currently wired
   `addScoreToTrace(...)` callback when possible
6. reduce that shape to the smallest honest external-consumer artifact

Discovery is only done when we have:

- one captured real exporter callback payload
- explicit note on whether the capture came from `onScoreEvent` or
  `addScoreToTrace(...)`
- one presence/absence table for the fields we call required vs optional
- confirmation that the required sample fields are not just guessed from docs
- at least one negative example showing an optional field truly absent, such as
  missing `spanId` or missing `metadata`

## 9.1 Exit criterion for P14

`P14` is not actually closed just because the current sample is smaller and
cleaner.

This lane is only complete when all of the following are true:

- one live exporter callback payload has been captured from a real Mastra run
- the capture path is named explicitly: `onScoreEvent` or
  `addScoreToTrace(...)`
- the current required vs optional field split has been checked against that
  real capture
- the frozen fixtures, README, and plan have been updated if the live payload
  proves the current sample too rich or too thin
- the lane still stays score-event-first and does not widen into traces,
  dashboards, or broader observability payloads

Until then, `P14b` should be described as:

- maintainer-guided
- docs-backed
- type-backed where possible
- pre-proof on the live callback path

If that discovery pass is too heavy or too unstable, fall back to a frozen
artifact that is explicitly marked as:

- maintainer-guided
- docs-backed where possible
- typed and exporter-derived
- non-normative

## 10. Concrete repo deliverable

If this recut is accepted, the next implementation PR should add either:

- a new `examples/mastra-score-event-evidence/` sample

or explicitly replace the current `examples/mastra-scorer-evidence/` sample
with an exporter-first score-event shape.

Preferred path:

- keep the original scorer sample historical
- add a new score-event sample

Planned files:

- `examples/mastra-score-event-evidence/README.md`
- `examples/mastra-score-event-evidence/map_to_assay.py`
- `examples/mastra-score-event-evidence/fixtures/valid.mastra.json`
- `examples/mastra-score-event-evidence/fixtures/failure.mastra.json`
- `examples/mastra-score-event-evidence/fixtures/malformed.mastra.json`
- `examples/mastra-score-event-evidence/fixtures/valid.assay.ndjson`
- `examples/mastra-score-event-evidence/fixtures/failure.assay.ndjson`

## 11. Generator policy

Preferred:

- a tiny real Mastra run with a custom exporter configured
- one real `ScoreEvent` / `ExportedScore` payload captured and then frozen

Fallback:

- one frozen typed artifact based on the discovery pass and maintainer
  guidance

Avoid:

- full Studio setup
- cloud-only dependencies
- tracing export as a shortcut

## 12. Valid, failure, malformed corpus

The first recut sample should still follow the established corpus pattern.

### 12.1 Valid

One score-event artifact with:

- one scorer id / name
- one bounded score
- one bounded target entity type

### 12.2 Failure

One weaker score artifact with:

- at least one scorer identity field still present
- lower score or bounded failure-class score label
- still a valid score event, not an infrastructure failure

### 12.3 Malformed

One malformed artifact that fails fast, for example:

- missing both `scorer_id` and `scorer_name`
- missing `score`
- long free-text reason body instead of bounded reason
- full trace payload smuggled into the sample
- free top-level `metadata` object instead of bounded `metadata_ref`

That last malformed case matters for product reasons, not just parser hygiene:

- Assay should not accept an arbitrary upstream bag as canonical top-level
  truth
- otherwise every new free-form metadata field would silently widen the claim
  surface
- `metadata_ref` keeps the possibility reviewable without pretending the
  metadata blob itself is part of the bounded evidence contract

## 13. Outward strategy

Do not open a new Mastra issue.

The outward route for `P14b` should stay inside the existing thread:

- build the recut sample on `main`
- reply in `mastra-ai/mastra#15206`
- acknowledge the exporter / score-event pivot
- ask one small follow-up question only if the sample still leaves seam
  ambiguity

Preferred follow-up question:

> We rebuilt the sample around one bounded `ScoreEvent` / `ExportedScore`
> artifact from the exporter path. Is there one field here you would drop or
> rename to keep the seam smaller and closer to the exporter payload?

## 14. Non-goals

This recut does not:

- define a trace adapter
- define a Studio adapter
- define an observability-wide export lane
- define experiment comparison semantics as Assay truth

## References

- [PLAN — P14 Mastra Scorer / Experiment-Result Evidence Interop](./PLAN-P14-MASTRA-SCORER-EXPERIMENT-RESULT-EVIDENCE-2026q2.md)
- [Mastra issue #15206](https://github.com/mastra-ai/mastra/issues/15206)
- [Maintainer exporter guidance on #15206](https://github.com/mastra-ai/mastra/issues/15206#issuecomment-4238852237)
- [Mastra observability](https://mastra.ai/observability)
- [Introducing Scorers in Mastra](https://mastra.ai/blog/mastra-scorers)
- [Change, Run, and Compare with Experiments in Mastra Studio](https://mastra.ai/blog/mastra-experiments)
- [Composite Storage with Mastra Storage](https://mastra.ai/blog/composite-storage-with-mastra-storage)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
