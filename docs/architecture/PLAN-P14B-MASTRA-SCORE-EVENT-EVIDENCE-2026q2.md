# PLAN — P14b Mastra ScoreEvent / ExportedScore Evidence Interop (2026 Q2)

- **Date:** 2026-04-15
- **Owner:** Evidence / Product
- **Status:** Docs-backed sample implementation merged; one local live
  `onScoreEvent` captured; capture-backed sample recut active
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

`target_ref` is an Assay-side bounded anchor derived from upstream exporter
anchors, not evidence that Mastra publishes one canonical `targetRef` export
field.

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

The current upstream picture is now clearer than it was during the first
re-cut.

What still holds:

- the score types define `ScoreEvent` and `ExportedScore`
- `ObservabilityEvents` exposes `onScoreEvent`
- the observability bus and several exporters already route score traffic
  through `onScoreEvent`

What has changed in our understanding:

- Mastra maintainers now explicitly point external consumers at
  `ObservabilityExporter` + `ScoreEvent` + `ExportedScore`
- Mastra maintainers also explicitly call `addScoreToTrace(...)` the old path
  and say it will be deprecated soon

Public Mastra references currently expose both the older
`addScoreToTrace(...)` hook shape and the newer `onScoreEvent(ScoreEvent)`
path; live callback capture is the tie-breaker for sample truth.

So `P14b` should now be framed as:

- **`ScoreEvent`-first by design**
- backed by one captured live callback, but not over-generalized beyond that
  single proof
- careful not to overread every richer typed field as already proven in one
  frozen external artifact

The older `addScoreToTrace(...)` path still matters only as migration context.
It explains why earlier code and docs looked thinner, but it is no longer the
seam this lane should bless going forward.

One local proof run now exists as well.

On 2026-04-15, a minimal local Node 22 harness using public Mastra packages
captured:

- exactly one real `onScoreEvent` payload
- exactly one legacy `addScoreToTrace(...)` call in the same run

That does two useful things for `P14b`:

- it proves the forward `ScoreEvent` path is live in a modern local run
- it also proves the legacy path still co-fires in at least one modern local
  run, so we should not write as if it has already disappeared

The captured `onScoreEvent` payload contained:

- `timestamp`
- `traceId`
- `spanId`
- `scorerId`
- `scoreSource`
- `score`
- `scoreTraceId`
- `correlationContext`
- `metadata`

The same real callback did **not** contain:

- `scorerName`
- `reason`
- top-level `targetEntityType`
- `scoreId`
- one native upstream `targetRef`

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

- `score_id_ref`
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

- the typed `ExportedScore` shape includes both identity concepts
- the current lane is only backed by one captured live callback
- the checked-in sample should not overclaim that one field is universally
  present until a real callback proves it

So the sample should require one bounded scorer identity, not pretend both are
already proven universal on the live `ScoreEvent` path.

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

The 2026-04-15 local `onScoreEvent` capture did not emit one native upstream
`targetRef` field. The checked-in sample therefore keeps `target_ref` as an
Assay-side bounded reduction over exporter anchors such as `spanId`,
`traceId`, and `correlationContext`, not as a claim about one official Mastra
target-ref export field.

#### `target_entity_type`

This field is optional in v1.

Why:

- the richer typed `ExportedScore` shape includes `targetEntityType`
- the first real callback did not prove that field present on the exact path we
  are targeting

The first local `onScoreEvent` capture exposed `correlationContext.entityType`
rather than one top-level `targetEntityType` field, which is another reason to
keep this optional and derived only when the reduction is still honest.

So this field is still useful when present, but it should not be a hard
required field until a real capture proves it is consistently emitted on the
path we are actually targeting.

#### `score_id_ref`

This field is optional in v1.

Mastra maintainers have now called out `ScoreId` as an upcoming addition to the
typed `ExportedScore` object, and it is likely to be the cleanest bounded
anchor back into Mastra's own score plane.

For Assay this should stay:

- opaque
- short
- anchor-only

Not allowed:

- score lookup URLs
- resolver paths
- embedded score payloads

The checked-in fixtures do not need to include this field yet. But the sample
contract should keep a bounded slot ready for it instead of pretending the
emerging anchor does not matter.

Treat `score_id_ref` as maintainer-guided and capture-pending until one real
exporter callback proves it live.

The 2026-04-15 local capture did not emit `scoreId`, so `score_id_ref`
remains optional and absent from the checked-in fixture corpus.

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

Before closing this lane, do one bounded discovery pass:

1. build a tiny real Mastra app with one scorer enabled
2. register a custom `ObservabilityExporter` implementing `onScoreEvent`
3. capture one real `ScoreEvent`
4. inspect the resulting `ExportedScore` shape
5. reduce that shape to the smallest honest external-consumer artifact
6. only keep any `addScoreToTrace(...)` note if a real run still shows it as
   historical compatibility context

Discovery is only done when we have:

- one captured real exporter callback payload
- explicit note that the capture came from `onScoreEvent`
- one presence/absence table for the fields we call required vs optional
- confirmation that the required sample fields are not just guessed from docs
- at least one negative example showing an optional field truly absent, such as
  missing `spanId` or missing `metadata`

## 9.1 Exit criterion for P14

`P14` is not actually closed just because the current sample is smaller and
cleaner.

This lane is only complete when all of the following are true:

- one live exporter callback payload has been captured from a real Mastra run
- the capture path is explicitly the typed `onScoreEvent` path
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
- backed by one live callback on the typed path, but still non-normative beyond
  that single proof

If that discovery pass is too heavy or too unstable, fall back to a frozen
artifact that is explicitly marked as:

- maintainer-guided
- docs-backed where possible
- typed and exporter-derived
- non-normative

## 9.2 Live capture objective

The next step is intentionally boring:

> capture one real `onScoreEvent` payload from a minimal local Mastra run, keep
> the raw payload intact, and compare it to the current frozen sample before we
> strengthen any upstream claim.

The goal is **not** to build a general Mastra adapter.

The goal is also **not** to prove every optional typed field.

The goal is only to answer these concrete questions:

- what exact object reaches `onScoreEvent` in a real local run
- which current sample fields are truly present vs merely type-visible
- whether one narrower or newer anchor such as `scoreId` is already live
- whether the current sample is too rich, too thin, or roughly right

### Capture result (2026-04-15)

That objective has now been completed once with a deliberately tiny local
harness:

- Node `22.22.2`
- `@mastra/core` `1.25.0`
- `@mastra/observability` `1.9.1`
- one agent
- one root-registered scorer
- one custom exporter implementing both `onScoreEvent(event)` and
  `addScoreToTrace(...)` for diagnostics only

Observed result:

- one real `onScoreEvent` payload was captured
- one legacy `addScoreToTrace(...)` call also fired in the same run
- the live callback was thinner than the richer frozen sample artifact

Presence / absence from that real `onScoreEvent` callback:

| Field | Seen in one local callback? | Notes |
| --- | --- | --- |
| `timestamp` | yes | top-level inside `score` |
| `traceId` | yes | live exporter anchor |
| `spanId` | yes | live exporter anchor |
| `scorerId` | yes | strongest scorer identity seen live |
| `scorerName` | no | not emitted in this run |
| `score` | yes | numeric |
| `reason` | no | not emitted in this run |
| `scoreSource` | yes | emitted as `live` |
| `scoreTraceId` | yes | live-only extra anchor, not yet used in the sample |
| `targetEntityType` | no | only `correlationContext.entityType` was present |
| `scoreId` | no | not emitted in this run |
| `metadata` | yes | free-form bag, kept out of canonical truth |
| `correlationContext` | yes | useful for reduction, not imported wholesale |
| native `targetRef` | no | Assay derives `target_ref` instead |

Capture-backed decision:

- keep the lane `ScoreEvent`-first
- keep `addScoreToTrace(...)` only as live co-fire migration context
- narrow the checked-in sample fixture set toward the thinner field profile
  actually seen live
- keep richer fields such as `reason`, `scorer_name`, and `score_id_ref`
  optional until another real callback proves them
- keep `target_ref` as a derived Assay anchor instead of pretending it is an
  upstream field

## 9.3 Minimal runtime target

Use the smallest local Mastra setup that can deterministically produce one
score event.

The preferred target shape is:

- one tiny local Mastra app
- one agent or workflow that can complete without cloud-only dependencies
- one scorer enabled
- one custom `ObservabilityExporter`
- one score-producing invocation

Hard constraints:

- local-only when possible
- no Studio dependency
- no observability sink beyond the custom exporter
- no full trace export
- no dashboard setup
- no multi-scorer matrix unless one scorer fails to emit

Preferred environment assumptions:

- Node 22.x
- the smallest Mastra package set needed to run one scored flow
- a pinned upstream commit or package version recorded in the capture notes

## 9.4 Capture harness shape

The harness should stay outside the canonical sample contract.

Treat it as a disposable proof tool, not as a new product surface.

Recommended harness pieces:

- one tiny Mastra app entrypoint
- one scorer configuration
- one `ObservabilityExporter` implementation with:
  - `onScoreEvent(event)`
  - a no-op tracing export method only if the interface requires it
- one file sink that writes the raw score payload exactly once
- one short run script that executes the scored path and exits cleanly

The exporter should write:

- the raw `event` payload as received
- a timestamp for the capture itself
- the exact Mastra version or commit under test
- the exact entrypoint used

The exporter should **not**:

- normalize fields before saving the raw capture
- drop unknown fields before saving
- enrich the payload with Assay wrappers
- emit traces, logs, or metrics into the same capture artifact

## 9.5 Proposed execution sequence

### Step 1 — Build the smallest runnable harness

Create a temporary local Mastra harness with one scorer and one exporter.

Success condition:

- the app starts
- one invocation path completes locally
- the exporter file sink is reachable

If this step fails because current Mastra setup is too heavy or unstable, stop
and record the blocker rather than widening the lane.

### Step 2 — Emit one real score event

Run the harness once with one deterministic-ish input that is known to trigger
the scorer.

Success condition:

- exactly one raw score-event payload is written
- the payload is clearly associated with `onScoreEvent`

If multiple score events fire, keep the first run but note the multiplicity.
Do not collapse or average the events at capture time.

### Step 3 — Freeze the raw capture

Preserve the raw payload exactly as emitted before any Assay-side reduction.

The raw capture should be saved separately from the sample fixture so we keep a
clear line between:

- upstream-emitted payload
- Assay-frozen external-consumer artifact

### Step 4 — Build a field presence table

From the raw payload, record a simple presence/absence table for:

- `scoreId`
- `scorerId`
- `scorerName`
- `score`
- `reason`
- `timestamp`
- `traceId`
- `spanId`
- `targetEntityType`
- `scoreSource`
- `scorerVersion`
- `metadata`
- any correlation / target anchor fields

This is the point where we decide what is:

- required in the sample
- optional in the sample
- still out of scope even if present

### Step 5 — Compare raw capture to the frozen sample

Compare the captured payload to the current sample contract with three
questions only:

- did we require anything that the live payload does not actually support
- did we omit one bounded field that is now clearly part of the live seam
- did we accidentally model any field as stronger than the live payload
  justifies

Do not turn this into a “how much more can we include” exercise.

### Step 6 — Re-cut only if evidence forces it

Allowed outcomes:

- no contract change needed
- one field becomes optional
- one field becomes newly available and bounded
- one field is renamed to stay closer to upstream reality

Not allowed:

- widening into traces
- widening into logs or metrics
- importing raw metadata blobs as truth
- inventing a broad Mastra export story from one successful run

## 9.6 Deliverables from the capture pass

The capture pass is only complete when it leaves behind:

- one raw captured `onScoreEvent` payload
- one short note describing the harness and Mastra version used
- one presence/absence table
- one written comparison against the current frozen sample
- one decision:
  - sample unchanged
  - sample narrowed
  - sample extended in one bounded way

If the raw payload cannot be safely checked into the repo, keep a redacted
internal note with the same field table and explicitly say what was redacted
and why.

## 9.7 Repo update plan after capture

Now that one local capture exists, the follow-up change in Assay should stay
very small.

Allowed repo updates:

- tweak fixture fields
- tighten README wording
- tighten or narrow required vs optional fields
- add one new bounded optional anchor such as `score_id_ref` if the live
  payload proves it
- add one short note saying the sample is now backed by one real callback
  capture

Avoid:

- a second large plan rewrite
- broad adapter work
- a new outward post before the sample comparison is finished

## 9.8 Stop conditions

Stop and reassess if any of these happen:

- the smallest local harness still requires broad observability or Studio setup
- `onScoreEvent` does not fire in a modern local run and only legacy pathways
  do
- the live payload shape differs so much from the frozen sample that a small
  bounded recut is no longer honest
- the only reliable capture path requires us to pull in traces or other broad
  observability payloads

If we hit one of those, the next action should be a short internal note and, if
needed, one very small outward clarification question. It should not be a
silent broadening of the lane.

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

- one scorer id
- one bounded score
- one bounded derived target anchor
- optional trace/span refs when they naturally exist

### 12.2 Failure

One weaker score artifact with:

- the same thin field profile as the valid artifact where possible
- at least one scorer identity field still present
- lower score
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
- [Maintainer note on `ScoreEvent` as the new path and upcoming `ScoreId`](https://github.com/mastra-ai/mastra/issues/15206#issuecomment-4252212575)
- [Mastra observability](https://mastra.ai/observability)
- [Introducing Scorers in Mastra](https://mastra.ai/blog/mastra-scorers)
- [Change, Run, and Compare with Experiments in Mastra Studio](https://mastra.ai/blog/mastra-experiments)
- [Composite Storage with Mastra Storage](https://mastra.ai/blog/composite-storage-with-mastra-storage)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
