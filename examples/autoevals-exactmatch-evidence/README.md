# AutoEvals ExactMatch Evidence Sample

This example turns one tiny frozen artifact derived from AutoEvals'
deterministic `ExactMatch` scorer into bounded, reviewable external evidence
for Assay.

It is intentionally small:

- start with one reduced artifact derived from one returned `Score` object
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep raw compared values, Braintrust logging, scorer config, metadata, and
  model-backed scorer truth out of Assay truth

## What is in here

- `capture_probe.py`: runs one positive and one negative `ExactMatch`
  evaluation and saves raw discovery artifacts
- `requirements.txt`: local probe dependency for the checked-in scorer path
- `discovery/valid.scorer.inputs.json`: the exact caller-side inputs used for
  the valid exact-match capture
- `discovery/valid.returned.score.json`: the raw returned `Score` object for
  the valid exact-match capture
- `discovery/failure.scorer.inputs.json`: the exact caller-side inputs used for
  the negative exact-match capture
- `discovery/failure.returned.score.json`: the raw returned `Score` object for
  the negative exact-match capture
- `discovery/FIELD_PRESENCE.md`: input-vs-returned notes and reduction
  rationale
- `map_to_assay.py`: turns one reduced AutoEvals `ExactMatch` artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.autoevals.json`: one bounded positive artifact derived from
  the returned score
- `fixtures/failure.autoevals.json`: one bounded negative artifact derived from
  the returned score
- `fixtures/malformed.autoevals.json`: one malformed input-plus-metadata import
  case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this surface

AutoEvals has a much broader public surface than this sample uses:

- LLM judge scorers
- RAG scorers
- JSON and list scorers
- Braintrust experiment logging
- model-provider-backed evaluation

This sample starts on the smaller deterministic scorer surface:

- one `ExactMatch` scorer call
- one returned `Score` object
- one bounded result bag

That keeps the first wedge smaller than:

- raw output/expected truth
- Braintrust run or experiment wrappers
- scorer-family truth
- prompt or model state
- broader AutoEvals platform truth

## Live discovery note

This sample is grounded in a small local probe run on **2026-04-24** against
`autoevals==0.2.0`.

The important boundary is now clear:

- scorer inputs are not the same thing as the returned public score
- the returned public score is a small `Score` object
- the raw returned object contains `name`, `score`, `metadata`, and `error`
- only `name` and integer `score` are needed for the v1 canonical artifact
- `metadata` stays out because `ExactMatch` returned an empty object
- `error` stays out because the successful score returned `null`

That is why the reduced artifact is derived from the returned score object
rather than from:

- the caller-side compared values
- Braintrust experiment wrappers
- scorer configuration
- docs snippets alone

For the reduced artifact:

- `scorer_name` is reduced from returned `name`
- `result.score` is copied from returned integer `score`
- `target_kind` is fixed to `output_expected_pair` because that is the level
  being compared, not because a stable comparison id was returned
- raw `output`, raw `expected`, raw `metadata`, raw `error`, and any scorer
  config fields stay out of the canonical artifact

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded negative
evaluation artifact, not an infrastructure failure.

## Re-run the local discovery probe

```bash
python3.12 -m venv /tmp/p27-autoevals-venv
/tmp/p27-autoevals-venv/bin/python -m pip install --upgrade pip
/tmp/p27-autoevals-venv/bin/python -m pip install \
  -r examples/autoevals-exactmatch-evidence/requirements.txt
/tmp/p27-autoevals-venv/bin/python \
  examples/autoevals-exactmatch-evidence/capture_probe.py
```

The probe prints the observed package version and writes raw input plus returned
artifacts into `discovery/`.

## Map the checked-in valid artifact

```bash
python3 examples/autoevals-exactmatch-evidence/map_to_assay.py \
  examples/autoevals-exactmatch-evidence/fixtures/valid.autoevals.json \
  --output examples/autoevals-exactmatch-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-24T10:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/autoevals-exactmatch-evidence/map_to_assay.py \
  examples/autoevals-exactmatch-evidence/fixtures/failure.autoevals.json \
  --output examples/autoevals-exactmatch-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-24T10:01:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/autoevals-exactmatch-evidence/map_to_assay.py \
  examples/autoevals-exactmatch-evidence/fixtures/malformed.autoevals.json \
  --output /tmp/autoevals-malformed.assay.ndjson \
  --import-time 2026-04-24T10:02:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry raw compared values, raw metadata, and a boolean score into a single-score
v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat AutoEvals or Braintrust evaluation semantics as Assay truth
- import raw outputs or expected values
- import scorer config, prompts, model, provider, context, or metadata
- partially import larger evaluation wrappers
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest AutoEvals `ExactMatch` score surface,
not a broad AutoEvals or Braintrust lane.
