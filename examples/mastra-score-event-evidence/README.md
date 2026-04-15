# Mastra ScoreEvent / ExportedScore Evidence Sample

This example turns one tiny frozen artifact derived from Mastra's typed
score-event exporter path into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start from one typed score-event seam
- keep the sample to one strong score artifact, one weak score artifact, and
  one malformed case
- map the two good artifacts into Assay-shaped placeholder envelopes
- keep scorer identity, numeric score, one derived target anchor, and
  timestamp at the center
- treat trace/span anchors as optional refs only
- keep traces, dashboards, broader observability payloads, and metadata blobs
  out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Mastra score artifact into an
  Assay-shaped placeholder envelope
- `score_event_exporter.example.ts`: small exporter-side sketch centered on the
  typed `onScoreEvent` path, with a legacy `addScoreToTrace(...)` note kept
  only as migration context
- `fixtures/valid.mastra.json`: one higher-score artifact
- `fixtures/failure.mastra.json`: one lower-score artifact that intentionally
  uses the thinner scorer-name-only shape
- `fixtures/malformed.mastra.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample follows the maintainer-guided Mastra recut:

- `ObservabilityExporter` is the narrow integration point
- `ScoreEvent` / `ExportedScore` is the primary typed score seam
- older `addScoreToTrace(...)` mentions are legacy context, not the forward
  seam this sample is trying to bless

## Current upstream seam

This sample models the current score-export reality as carefully as we can see
it today, but it is now deliberately `ScoreEvent`-first.

What that means in practice:

- current Mastra observability code and maintainer guidance both point toward
  `ScoreEvent` / `ExportedScore`
- some older hooks, docs, and exporter helpers still mention
  `addScoreToTrace(...)`
- Mastra has now explicitly called `addScoreToTrace(...)` the old path and
  said it will be deprecated soon
- this sample still does **not** claim that every live `onScoreEvent` callback
  will match the exact frozen fixture shape checked in here

So this is a bounded mapping lane for the `ScoreEvent` exporter reality we are
targeting, not a claim that every live Mastra score callback has already been
proven against the frozen fixture shape.

## Live callback proof status

On 2026-04-15 we captured one real local `onScoreEvent` from a deliberately
tiny Mastra harness.

That same run also emitted one legacy `addScoreToTrace(...)` call, so the live
story is now a bit more precise than the docs alone:

- the forward typed `ScoreEvent` path is real in a modern local run
- the older legacy callback can still co-fire in the same run

The real `onScoreEvent` payload was thinner than the richer frozen sample
artifact we had before. In that callback we observed:

- `timestamp`
- `traceId`
- `spanId`
- `scorerId`
- `scoreSource`
- `score`
- `scoreTraceId`
- `correlationContext`
- `metadata`

And we did **not** observe:

- `scorerName`
- `reason`
- top-level `targetEntityType`
- `scoreId`
- one native upstream `targetRef`

That is why the checked-in valid and failure fixtures now stay close to the
thinner field set actually seen in one live callback. They are still bounded
derived external-consumer artifacts, not raw callback dumps.

In particular:

- `target_ref` is an Assay-side bounded reduction over exporter anchors such as
  `spanId`, `traceId`, and `correlationContext`
- `score_id_ref` stays optional and absent from the checked-in fixtures until a
  real callback proves it live
- `reason` and `scorer_name` remain allowed by the sample contract, but they
  are no longer baked into the checked-in fixtures without live proof

## Terminology alignment

Mastra's public exporter story is now best read through `ScoreEvent` carrying
`ExportedScore`.

The older `addScoreToTrace(...)` payload still matters only as historical
context, because it explains why some older code and docs looked thinner than
the typed score event seam.

That leads to one careful but useful distinction:

- the score types define `ScoreEvent` and `ExportedScore`
- `ObservabilityEvents` exposes `onScoreEvent`
- `addScoreToTrace(...)` still exists in some codepaths and docs
- but Mastra now calls that the old path and points external consumers to
  `ScoreEvent` instead

So this sample is maintainer-guided and type-backed around `ScoreEvent`, and it
is now backed by one real live callback without pretending that one proof makes
the fixture shape universal.

This sample uses both names carefully:

- `score_id_ref` is reserved for Mastra's upcoming `ScoreId` anchor when that
  field lands on `ExportedScore`
- `trace_id_ref` maps to `traceId`
- `span_id_ref` maps to `spanId`
- `score` maps to `score`
- `reason` maps to `reason`
- `scorer_name` maps to `scorerName`
- `metadata_ref` is a bounded reference standing in for `metadata`
- `target_ref` is a sample-level bounded anchor derived from the exporter
  payload, not a claim that upstream publishes one official `targetRef` field

The checked-in fixtures do not yet include `score_id_ref`, because the first
live callback did not carry that field. But the sample now keeps the slot
bounded and ready instead of pretending the new anchor does not exist.

One small but important distinction:

- `scorer_id` is the stronger identity field when present
- `scorer_name` is still sufficient for this sample, but it is closer to a
  display identity than a guaranteed stable upstream identifier

That is why the contract requires at least one scorer identity field, but does
not pretend they carry the same strength.

That keeps the first slice score-event-first. It does not turn the sample
into:

- a trace export lane
- a Studio/dashboard lane
- a full observability sink
- a runtime correctness lane

The checked-in fixtures are deliberately frozen and non-normative. They are a
small external-consumer sample derived from the current score exporter path,
not a claim that Mastra already guarantees one canonical export wrapper for all
consumers.

For the same reason, `target_entity_type` stays a bounded optional classifier
in this sample rather than pretending Assay now owns Mastra's full internal
entity model.

The checked-in sample also makes one v1 choice explicit: `score` is always
numeric here. We are not treating this lane as a generic categorical judgment
surface.

## Small exporter sketch

The sample also includes one tiny exporter-side sketch in
`score_event_exporter.example.ts`. It is not a production adapter. It only
shows the smallest part of the exporter path we care about:

- receive `ScoreEvent` on the primary typed path
- keep one historical `addScoreToTrace(...)` sketch only as a migration note
- project one bounded frozen artifact for external evidence

## Map the checked-in valid artifact

```bash
python3 examples/mastra-score-event-evidence/map_to_assay.py \
  examples/mastra-score-event-evidence/fixtures/valid.mastra.json \
  --output examples/mastra-score-event-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-14T10:00:00Z \
  --overwrite
```

## Map the checked-in lower-score artifact

```bash
python3 examples/mastra-score-event-evidence/map_to_assay.py \
  examples/mastra-score-event-evidence/fixtures/failure.mastra.json \
  --output examples/mastra-score-event-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-14T10:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/mastra-score-event-evidence/map_to_assay.py \
  examples/mastra-score-event-evidence/fixtures/malformed.mastra.json \
  --output /tmp/mastra-score-event-malformed.assay.ndjson \
  --import-time 2026-04-14T10:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
inline a `metadata` object instead of keeping metadata bounded behind a ref.

That failure is intentional for product reasons, not just parser hygiene:

- we do not accept a free top-level upstream bag into the canonical sample
  contract
- otherwise the claim surface would silently widen every time a new metadata
  field appeared
- `metadata_ref` keeps that possibility reviewable without treating arbitrary
  metadata as truth

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Mastra scoring semantics, trace semantics, or observability semantics
  as Assay truth
- imply that Assay independently verified a Mastra runtime outcome
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest Mastra score-export surface, not a
trace, dashboard, or observability-wide export surface.

Additional caps in this sample:

- at least one scorer identity field must be present: `scorer_id` or
  `scorer_name`
- `score_id_ref`, `target_ref`, `trace_id_ref`, `span_id_ref`, and
  `metadata_ref` must stay opaque ids, not URLs
- `reason` must stay short and single-line
- `score` stays numeric in v1

We are not asking Assay to inherit Mastra scoring semantics, observability
semantics, or runtime semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.mastra.json`: bounded score-event artifact with a stronger
  score
- `fixtures/failure.mastra.json`: bounded score artifact with a weaker score
  and only the scorer-name identity field
- `fixtures/malformed.mastra.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
