# Pydantic AI Eval Report Evidence Sample

This example turns one tiny serialized artifact derived from a documented
`EvaluationReport` result surface into bounded, reviewable external evidence
for Assay.

> **Current status:** this is the original P9 report-wrapper discovery sample.
> The next planned slice is
> [P9b](../../docs/architecture/PLAN-P9B-PYDANTIC-REPORTCASE-RESULT-EVIDENCE-2026q2.md),
> which should recut the lane around one reduced case-result artifact derived
> from `EvaluationReport.cases[]`. Treat `ReportCase` as discovery input and
> treat the report-wrapper fixtures here as historical discovery, not the
> target importer shape.

It is intentionally small:

- start with one local `evaluate_sync()` / `EvaluationReport` flow
- freeze one valid artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep per-case statuses and scores as observed eval-report data only
- keep evaluator truth, tracing semantics, and runtime semantics out of Assay truth

## What is in here

- `generate_synthetic_report.py`: runs a tiny local `pydantic_evals` dataset
  and writes a smaller serialized artifact derived from the resulting
  `EvaluationReport`
- `map_to_assay.py`: turns that frozen eval-report artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.pydantic-ai.json`: one completed eval-report artifact with
  all cases passing
- `fixtures/failure.pydantic-ai.json`: one completed eval-report artifact with
  one passing case and one failing case
- `fixtures/malformed.pydantic-ai.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed import time

## Why this seam

This sample treats a serialized artifact derived from the documented
`EvaluationReport` result surface as the current best first seam hypothesis for
`pydantic-ai` / `pydantic_evals`.

That keeps the first slice on code-first eval artifacts only. It does not turn
the sample into:

- a Logfire export lane
- an OpenTelemetry export lane
- a span-based evaluator lane
- an agent runtime trace lane
- a generalized observability sink

The generator uses a tiny local `Dataset(...).evaluate_sync(...)` flow and then
freezes a smaller export shape. That export shape is intentionally narrower
than the full upstream report object.

The checked-in fixtures also omit `trace_ref` on purpose. `trace_ref` may exist
later as an opaque reference, but v1 keeps the sample on eval-result artifacts
only.

The checked-in artifact also includes a small `report_id` added by the local
export step. That field is generator-side export metadata for the frozen sample;
it is not a claim that upstream already guarantees a wire-format contract with
that identifier.

## Install the tiny local generator dependency

```bash
python3 -m venv /tmp/pydantic-ai-eval-sample-venv
source /tmp/pydantic-ai-eval-sample-venv/bin/activate
python -m pip install -r examples/pydantic-ai-eval-report-evidence/requirements.txt
```

## Generate the checked-in valid artifact

```bash
source /tmp/pydantic-ai-eval-sample-venv/bin/activate
python examples/pydantic-ai-eval-report-evidence/generate_synthetic_report.py \
  --scenario valid \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/valid.pydantic-ai.json \
  --timestamp 2026-04-07T19:00:00Z \
  --overwrite
```

## Generate the checked-in failure artifact

```bash
source /tmp/pydantic-ai-eval-sample-venv/bin/activate
python examples/pydantic-ai-eval-report-evidence/generate_synthetic_report.py \
  --scenario failure \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/failure.pydantic-ai.json \
  --timestamp 2026-04-07T19:05:00Z \
  --overwrite
```

## Map the checked-in valid artifact

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/valid.pydantic-ai.json \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T19:30:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/failure.pydantic-ai.json \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T19:35:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/malformed.pydantic-ai.json \
  --output /tmp/pydantic-ai-malformed.assay.ndjson \
  --import-time 2026-04-07T19:40:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat report judgments, evaluator semantics, or tracing semantics as Assay truth
- imply that Assay independently verified model quality or evaluator correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest evaluation-report surface exposed by a
code-first eval framework, not a tracing backend, observability sink, or
runtime truth API.

We are not asking Assay to inherit `pydantic_evals` report judgments,
evaluator semantics, or tracing semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, LangGraph, OpenAI Agents,
MAF, A2A, and UCP samples, so the placeholder envelopes are honest about
deterministic hashing without pretending to be a full RFC 8785 canonicalizer
for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.pydantic-ai.json`: bounded eval-report export with all cases passing
- `fixtures/failure.pydantic-ai.json`: bounded eval-report export with one failing case
- `fixtures/malformed.pydantic-ai.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
