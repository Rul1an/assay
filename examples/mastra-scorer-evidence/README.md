# Mastra Scorer Evidence Sample

This example turns one tiny frozen artifact derived from Mastra's earlier
scorer / experiment workflow seam hypothesis into bounded, reviewable external
evidence for Assay.

It is kept as a historical comparison point. The newer Mastra recut lives in
`../mastra-score-event-evidence/` and follows the maintainer-guided
`ObservabilityExporter` / `ScoreEvent` path instead.

It is intentionally small:

- start with one frozen scorer-result artifact shape
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep score and experiment context as observed upstream data only
- keep tracing, Studio metrics, dashboard semantics, and runtime truth out of
  Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Mastra scorer artifact into an Assay-shaped
  placeholder envelope
- `fixtures/valid.mastra.json`: one stronger scorer artifact
- `fixtures/failure.mastra.json`: one weaker scorer artifact
- `fixtures/malformed.mastra.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from Mastra's earlier
scorer and experiment path as the first seam hypothesis we tested for Mastra.

That keeps the first slice on scorer-result artifacts only. It does not turn
the sample into:

- a trace export lane
- a Studio metrics export lane
- a dashboard export lane
- a legacy evals lane
- a generalized observability sink

The checked-in fixtures are deliberately docs-backed and frozen. Mastra has
real scorer, dataset, and experiment surfaces, but v1 keeps the evidence
boundary honest without pretending that a live framework bootstrap is already
the smallest stable path.

The checked-in fixtures also omit `run_ref`, `target_ref`, `scorer_reason_ref`,
and every other optional top-level reference on purpose. Those fields may
arrive later in a bounded sample shape, but v1 keeps the seam on one scorer
name, one bounded score, one experiment label, one dataset version reference,
and one bounded item reference.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a weaker scorer result,
not a platform failure or infrastructure failure.

## Map the checked-in valid artifact

```bash
python3 examples/mastra-scorer-evidence/map_to_assay.py \
  examples/mastra-scorer-evidence/fixtures/valid.mastra.json \
  --output examples/mastra-scorer-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-09T09:15:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/mastra-scorer-evidence/map_to_assay.py \
  examples/mastra-scorer-evidence/fixtures/failure.mastra.json \
  --output examples/mastra-scorer-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-09T09:20:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/mastra-scorer-evidence/map_to_assay.py \
  examples/mastra-scorer-evidence/fixtures/malformed.mastra.json \
  --output /tmp/mastra-malformed.assay.ndjson \
  --import-time 2026-04-09T09:25:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Mastra scorer semantics, experiment semantics, tracing semantics, or
  Studio metrics semantics as Assay truth
- imply that Assay independently verified runtime correctness or model quality
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest Mastra scorer-result surface, not a
trace export, observability export, dashboard export, or runtime truth
surface.

We are not asking Assay to inherit Mastra scorer semantics, experiment
semantics, dashboard semantics, or observability semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.mastra.json`: bounded scorer artifact with a stronger score
- `fixtures/failure.mastra.json`: bounded scorer artifact with a weaker score
- `fixtures/malformed.mastra.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
