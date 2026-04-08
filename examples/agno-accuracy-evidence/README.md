# Agno Accuracy Eval Evidence Sample

This example turns one tiny artifact derived from the documented Agno
`AccuracyEval` / `AccuracyResult` surface into bounded, reviewable external
evidence for Assay.

It is intentionally small:

- start with one frozen eval-result artifact shape derived from the public
  Accuracy docs
- keep the sample to one valid artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep scores, average score, and outcome as observed eval-result data only
- keep tracing, AgentOS platform semantics, and evaluator truth out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Agno accuracy artifact into an Assay-shaped
  placeholder envelope
- `fixtures/valid.agno.json`: one completed accuracy-eval artifact
- `fixtures/failure.agno.json`: one failed accuracy-eval artifact
- `fixtures/malformed.agno.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed import time

## Why this seam

This sample treats a frozen serialized artifact derived from the documented
`AccuracyEval` / `AccuracyResult` surface as the current best first seam
hypothesis for Agno.

That keeps the first slice on eval-result artifacts only. It does not turn the
sample into:

- a tracing export lane
- an AgentOS eval-run API lane
- an `AgentAsJudgeEval` lane
- a performance or reliability eval lane
- a generalized observability sink

The checked-in fixtures are deliberately docs-backed and frozen. The public
Agno accuracy examples rely on model-backed eval configuration, so this sample
keeps the evidence seam honest without pretending that a zero-credential local
generator is already the smallest stable path.

The checked-in fixtures also omit `threshold` and every optional reference on
purpose. Those fields may exist later in a bounded sample shape, but v1 keeps
the seam on scores, average score, iterations, and outcome only.

## Map the checked-in valid artifact

```bash
python3 examples/agno-accuracy-evidence/map_to_assay.py \
  examples/agno-accuracy-evidence/fixtures/valid.agno.json \
  --output examples/agno-accuracy-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-08T09:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/agno-accuracy-evidence/map_to_assay.py \
  examples/agno-accuracy-evidence/fixtures/failure.agno.json \
  --output examples/agno-accuracy-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-08T09:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/agno-accuracy-evidence/map_to_assay.py \
  examples/agno-accuracy-evidence/fixtures/malformed.agno.json \
  --output /tmp/agno-malformed.assay.ndjson \
  --import-time 2026-04-08T09:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Agno eval judgments, evaluator semantics, or tracing semantics as Assay truth
- imply that Assay independently verified runtime correctness or evaluator correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest eval-result surface exposed by Agno,
not a tracing export, AgentOS platform API, or runtime truth surface.

We are not asking Assay to inherit Agno eval judgments, evaluator semantics,
runtime semantics, or tracing semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, LangGraph, OpenAI Agents,
MAF, A2A, UCP, and Pydantic AI samples, so the placeholder envelopes are
honest about deterministic hashing without pretending to be a full RFC 8785
canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.agno.json`: bounded valid accuracy artifact
- `fixtures/failure.agno.json`: bounded failure accuracy artifact
- `fixtures/malformed.agno.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
