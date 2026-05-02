# Pydantic Evals Reduced Case-Result Evidence Sample

This example turns one tiny artifact derived from
`EvaluationReport.cases[]` into bounded, reviewable external evidence for
Assay.

The important cut is deliberate: `ReportCase` is the discovery source, not the
artifact contract. A live inspection against `pydantic-evals==1.89.1` shows
that report cases can carry broad fields such as `inputs`, `expected_output`,
`output`, `trace_id`, and `span_id`. The checked-in v1 artifact excludes those
fields and keeps only the reduced case-result boundary.

It is intentionally small:

- start with one local `Dataset(...).evaluate_sync(...)` /
  `EvaluationReport` flow
- derive one reduced case-result artifact from one case entry
- keep `case_name` as the docs-backed case identity
- keep bounded assertion/score result values with evaluator names
- reject raw task inputs, expected outputs, model outputs, trace/span payloads,
  report summaries, and Logfire context

## What is in here

- `generate_synthetic_report.py`: runs a tiny local `pydantic_evals` dataset
  and writes one reduced case-result artifact derived from
  `EvaluationReport.cases[]`
- `map_to_assay.py`: validates that reduced artifact and turns it into an
  Assay-shaped placeholder envelope
- `fixtures/valid.pydantic-ai.json`: one passing reduced case-result artifact
- `fixtures/failure.pydantic-ai.json`: one failing reduced case-result artifact
- `fixtures/malformed.pydantic-ai.json`: one intentionally broad artifact that
  tries to include raw `expected_output` / `output`
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Reduced Artifact Shape

The checked-in artifact is not a raw `ReportCase` object and not a full
`EvaluationReport`.

```json
{
  "schema": "pydantic-evals.report-case-result.export.v1",
  "framework": "pydantic_evals",
  "surface": "evaluation_report.cases.case_result",
  "case_name": "case-hello",
  "results": [
    {
      "evaluator_name": "EqualsExpected",
      "kind": "assertion",
      "passed": true
    },
    {
      "evaluator_name": "ExactScorePoints",
      "kind": "score",
      "score": 1.0
    }
  ],
  "timestamp": "2026-05-02T08:00:00Z"
}
```

Field status:

- docs-backed: `EvaluationReport`, `EvaluationReport.cases[]`, `case_name`
- live-backed in `pydantic-evals==1.89.1`: assertion result names/values and
  score result names/values inside the dumped report case
- downstream export metadata: `timestamp`
- deliberately absent: `case_id_ref`, report summary, report name, dataset
  metadata, task input, expected output, model output, traces, spans, and
  Logfire payloads

## Why this seam

This sample treats a reduced case-result artifact derived from
`EvaluationReport.cases[]` as the current best first seam hypothesis for
`pydantic-ai` / `pydantic_evals`.

That keeps the lane on code-first eval artifacts only. It does not turn the
sample into:

- a Logfire export lane
- an OpenTelemetry export lane
- a span-based evaluator lane
- an agent runtime trace lane
- a generalized observability sink
- a Trust Basis claim or public receipt family

The mapper writes sample-only placeholder envelopes. It does not register a
new Assay Evidence Contract event type and does not claim that Assay verified
model quality, evaluator correctness, or upstream runtime semantics.

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
  --timestamp 2026-05-02T08:00:00Z \
  --overwrite
```

## Generate the checked-in failure artifact

```bash
source /tmp/pydantic-ai-eval-sample-venv/bin/activate
python examples/pydantic-ai-eval-report-evidence/generate_synthetic_report.py \
  --scenario failure \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/failure.pydantic-ai.json \
  --timestamp 2026-05-02T08:05:00Z \
  --overwrite
```

## Map the checked-in valid artifact

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/valid.pydantic-ai.json \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-05-02T08:30:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/failure.pydantic-ai.json \
  --output examples/pydantic-ai-eval-report-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-05-02T08:35:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/pydantic-ai-eval-report-evidence/map_to_assay.py \
  examples/pydantic-ai-eval-report-evidence/fixtures/malformed.pydantic-ai.json \
  --output /tmp/pydantic-ai-malformed.assay.ndjson \
  --import-time 2026-05-02T08:40:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture includes
unsupported broad case fields: `expected_output` and `output`.

## Important Boundary

We are not asking Assay to inherit `pydantic_evals` report judgments,
evaluator semantics, or tracing semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.
