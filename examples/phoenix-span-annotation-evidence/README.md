# Phoenix Span Annotation Evidence Sample

This example turns one tiny frozen artifact derived from Phoenix's span
annotation retrieve path into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with one reduced artifact derived from one retrieved span annotation
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep `annotator_kind` as observed provenance only
- keep raw metadata, trace trees, experiments, datasets, and broader Phoenix
  platform truth out of Assay truth

## What is in here

- `capture_probe.py`: small local Phoenix probe that creates a trace span, adds
  annotations, and prints raw create/retrieve payloads
- `requirements.txt`: lightweight local probe dependencies
- `discovery/valid.create.request.json`: the public create-shape we sent for
  the valid live annotation
- `discovery/valid.create.response.json`: the raw create response from Phoenix
- `discovery/valid.retrieve.response.json`: the raw retrieve response from
  Phoenix
- `discovery/failure.create.request.json`: the public create-shape we sent for
  the failure live annotation
- `discovery/failure.create.response.json`: the raw create response from Phoenix
- `discovery/failure.retrieve.response.json`: the raw retrieve response from
  Phoenix
- `discovery/FIELD_PRESENCE.md`: create-vs-retrieve notes and reduction
  rationale
- `map_to_assay.py`: turns one reduced Phoenix span annotation artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.phoenix.json`: one bounded span annotation artifact derived
  from the live retrieve path
- `fixtures/failure.phoenix.json`: one bounded negative span annotation artifact
  derived from the live retrieve path
- `fixtures/malformed.phoenix.json`: one malformed batch-wrapper import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this seam

Phoenix is a large platform with tracing, evaluation, datasets, experiments,
playground, and prompt management surfaces.

This sample intentionally does **not** start there.

It starts on the smaller public annotation seam that Phoenix already documents:

- one target entity id
- one annotation `name`
- one small `result` bag with `label`, `score`, and optional `explanation`
- optional bounded provenance such as `annotator_kind` and `identifier`

That keeps the first wedge smaller than:

- full traces
- span trees
- experiment wrappers
- dataset payloads
- prompt or dashboard state

## Live discovery note

This sample is stronger than docs-only.

It is grounded in a small live local probe against Phoenix on **2026-04-22**:

- one real OTLP span was sent to a local Phoenix instance
- one positive span annotation was created and retrieved
- one negative span annotation was created and retrieved

The useful shape boundary is clear:

- the create response only returned an inserted annotation id
- the retrieve response returned the actual annotation body
- when `identifier` and `metadata` were omitted on create, the retrieve path
  still materialized them as `""` and `{}` respectively

That is why the checked-in sample is retrieve-derived rather than create-derived,
and why the reduced artifact explicitly normalizes empty optionals away instead
of pretending they are meaningful first-class evidence.

For the reduced artifact:

- `timestamp` is taken from the retrieved annotation's `updated_at`
- raw `id`, `source`, `user_id`, `created_at`, `updated_at`, and raw metadata
  stay out of the canonical artifact
- raw metadata is intentionally omitted rather than imported inline

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded annotation
artifact with a low score / negative label, not an infrastructure failure.

## Re-run the local discovery probe

Start a local Phoenix server first. The smallest local path is:

```bash
docker run --rm -p 6006:6006 -p 4317:4317 arizephoenix/phoenix:latest
```

Then run:

```bash
python3 -m venv /tmp/p24-phoenix-venv
source /tmp/p24-phoenix-venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r examples/phoenix-span-annotation-evidence/requirements.txt
python examples/phoenix-span-annotation-evidence/capture_probe.py
```

The probe prints raw create and retrieve payloads for the live span annotations.

## Map the checked-in valid artifact

```bash
python3 examples/phoenix-span-annotation-evidence/map_to_assay.py \
  examples/phoenix-span-annotation-evidence/fixtures/valid.phoenix.json \
  --output examples/phoenix-span-annotation-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-22T17:20:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/phoenix-span-annotation-evidence/map_to_assay.py \
  examples/phoenix-span-annotation-evidence/fixtures/failure.phoenix.json \
  --output examples/phoenix-span-annotation-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-22T17:25:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/phoenix-span-annotation-evidence/map_to_assay.py \
  examples/phoenix-span-annotation-evidence/fixtures/malformed.phoenix.json \
  --output /tmp/phoenix-malformed.assay.ndjson \
  --import-time 2026-04-22T17:30:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry a batch annotation wrapper into a single-annotation v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Phoenix annotation, evaluator, or platform semantics as Assay truth
- treat `annotator_kind` as evaluator-quality truth
- import raw trace trees, experiments, or dataset payloads
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest Phoenix annotation seam, not a Phoenix
platform or trace export lane.

We are not asking Assay to inherit Phoenix trace, experiment, evaluator, or
dashboard semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same small,
deterministic JSON profile used by the other interop samples. It is honest
about deterministic hashing for this sample corpus without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.phoenix.json`: bounded positive span annotation artifact
- `fixtures/failure.phoenix.json`: bounded negative span annotation artifact
- `fixtures/malformed.phoenix.json`: malformed batch-wrapper import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
