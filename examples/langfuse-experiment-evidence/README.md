# Langfuse Experiment Result Evidence Sample

This example turns one tiny frozen artifact derived from the documented
Langfuse experiment runner path into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start with one frozen experiment-item-result artifact shape derived from the
  public Langfuse experiments and evaluation docs
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep output and evaluations as observed experiment-result data only
- keep trace semantics, dashboard semantics, metrics semantics, and platform
  semantics out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Langfuse experiment-result artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.langfuse.json`: one strong experiment-item-result artifact
- `fixtures/failure.langfuse.json`: one lower-scoring experiment-item-result
  artifact
- `fixtures/malformed.langfuse.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from the documented
Langfuse experiment runner path as the current best first seam hypothesis for
Langfuse.

That keeps the first slice on experiment-result artifacts only. It does not
turn the sample into:

- a trace export lane
- a dashboard export lane
- a metrics export lane
- a prompt-management lane
- a generalized observability sink

The checked-in fixtures are deliberately docs-backed and frozen. Langfuse has
real experiment runner and evaluation surfaces, but the platform setup is still
larger than the seam we want to test here, so v1 keeps the evidence boundary
honest without pretending that a live platform bootstrap is already the
smallest stable path.

The checked-in fixtures also omit `trace_ref`, `aggregate_scores`, and every
optional top-level reference on purpose. Those fields may exist later in a
bounded sample shape, but v1 keeps the seam on one item result, one small
output reduction, one bounded dataset version reference, and one small
evaluation list.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a weaker experiment
result, not a platform failure or infrastructure failure.

## Map the checked-in valid artifact

```bash
python3 examples/langfuse-experiment-evidence/map_to_assay.py \
  examples/langfuse-experiment-evidence/fixtures/valid.langfuse.json \
  --output examples/langfuse-experiment-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-08T14:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/langfuse-experiment-evidence/map_to_assay.py \
  examples/langfuse-experiment-evidence/fixtures/failure.langfuse.json \
  --output examples/langfuse-experiment-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-08T14:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/langfuse-experiment-evidence/map_to_assay.py \
  examples/langfuse-experiment-evidence/fixtures/malformed.langfuse.json \
  --output /tmp/langfuse-malformed.assay.ndjson \
  --import-time 2026-04-08T14:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Langfuse trace semantics, dashboard semantics, metrics semantics, or
  evaluation semantics as Assay truth
- imply that Assay independently verified model quality, prompt quality, or
  platform correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest Langfuse experiment-result surface, not
a trace export, dashboard export, metrics export, or production observability
truth surface.

We are not asking Assay to inherit Langfuse trace semantics, dashboard
semantics, metrics semantics, or evaluation semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.langfuse.json`: bounded experiment-item-result artifact with
  stronger evaluation values
- `fixtures/failure.langfuse.json`: bounded experiment-item-result artifact
  with lower evaluation values
- `fixtures/malformed.langfuse.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
