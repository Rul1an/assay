# PLAN — P9 Pydantic AI Evaluation Report Evidence Interop (2026 Q2)

- **Date:** 2026-04-07
- **Owner:** Evidence / Product
- **Status:** Historical discovery; superseded for execution by
  [P9b](./PLAN-P9B-PYDANTIC-REPORTCASE-RESULT-EVIDENCE-2026q2.md)
- **Scope (this PR):** Define the next external interop lane after `mcp-agent`.
  No sample implementation, no outward post, no contract freeze in this slice.

> **2026-05-02 execution note:** The original P9 report-wrapper shape remains
> useful discovery, but it is too broad for the next slice. P9b narrows the
> execution candidate to one reduced case-result artifact derived from
> `EvaluationReport.cases[]`. `ReportCase` is discovery input, not the implied
> v1 contract unit. Possible importer-only support remains contingent on a
> successful live recut, with no report-wide summaries, analyses, trace/span
> metadata, Logfire payloads, Trust Basis claim, Harness recipe, or public
> story.

## 1. Why this plan exists

After the current framework, protocol, and runtime-accounting wave, the next
interop lane should still follow the same discipline:

1. pick one bounded surface the upstream project already exposes,
2. consume that surface without inheriting upstream semantics as Assay truth,
3. use the maintainer channel that actually matches the repo.

`pydantic/pydantic-ai` fits that pattern well:

- the repo is active and widely used
- the docs expose a code-first evals layer with a first-class `EvaluationReport`
- the docs also make clear that GitHub Issues and Slack are the natural help
  channels, not GitHub Discussions

This makes it a strong next candidate, but only if Assay chooses the smallest
honest eval artifact rather than sliding into another observability pitch.

This is **not** a Logfire-first plan.

This is **not** a span-based evaluation or OpenTelemetry-first plan.

This is a plan for a **bounded evaluation-report seam from a code-first eval
framework**.

## 2. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest evaluation-report surface exposed by
> a code-first eval framework, not a tracing backend, observability sink, or
> runtime truth API.

That means:

- `pydantic-ai` and `pydantic_evals` are the upstream context, not the truth source
- `EvaluationReport` is an eval result artifact, not a trace contract
- Assay stays an external evidence consumer, not a judge of evaluator
  correctness, model correctness, or telemetry truth

## 3. Why not Logfire or span-based evaluation first

`pydantic-ai` also exposes OpenTelemetry and Logfire integration, and
`pydantic_evals` supports span-based evaluation, but those are not the right
first wedge here.

The official docs are clear that:

- evals are code-first and produce a report object you can serialize and store
- tracing is optional and comes into the picture if Logfire or OpenTelemetry is
  configured
- GitHub Issues and Slack are the support/feedback channels

So the better first seam is smaller and more distinct:

- one frozen `EvaluationReport`-style artifact
- bounded per-case results
- bounded summary statistics
- optional performance data only if naturally present

This keeps the lane clearly different from:

- Microsoft Agent Framework trace export
- OpenAI Agents trace processors
- LangGraph tasks streams
- `mcp-agent` runtime accounting summaries

## 4. Recommended v1 seam

Use **one serialized `EvaluationReport`-style artifact from the documented
code-first evals path** as the first external-consumer seam.

This seam is:

- artifact-first
- code-first
- reviewable
- smaller than span-based or telemetry-first routes
- directly aligned with the upstream docs that describe a report object with
  per-case results and experiment-wide analyses

This is intentionally not:

- Logfire export
- OpenTelemetry export
- span-based evaluator output as the first seam
- an agent runtime trace
- a generalized observability event stream

## 5. v1 artifact contract

### 5.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `dataset_name`
- `experiment_name`
- `report_id`
- `timestamp`
- `outcome`
- `summary`
- `case_results`

### 5.2 Required summary fields

The first sample should require:

- `summary.case_count`
- `summary.pass_count`
- `summary.fail_count`

### 5.3 Required case result fields

Each case result in v1 should require:

- `case_id`
- `status`
- `scores`

### 5.4 Optional fields

The first sample may include:

- `summary.average_score`
- `duration_ms`
- `labels`
- `assertions`
- `trace_ref`

### 5.5 Important field boundaries

#### `summary.average_score`

This field is optional and secondary.

If present, it must be treated as an upstream aggregate metric, not as:

- quality truth
- model truth
- evaluator truth

If omitted, the sample remains fully valid.

#### `duration_ms`

This field is optional and observational only.

If present, it must not be promoted into:

- performance truth
- SLA truth
- production latency truth

#### `trace_ref`

`trace_ref` must stay a bounded reference only.

Allowed in v1:

- opaque identifier
- small URI or label

Not allowed in v1:

- trace payload
- span payload
- OpenTelemetry export
- Logfire event payloads
- span-based evaluator semantics promoted into evidence meaning

## 6. Assay-side meaning

The sample may only claim bounded eval-report observation.

Assay must not treat as truth:

- evaluator correctness
- model correctness
- pass/fail semantics beyond the observed upstream report
- performance truth
- tracing truth
- runtime semantics beyond the bounded observed artifact

Common anti-overclaim sentence:

> We are not asking Assay to inherit `pydantic_evals` report judgments,
> evaluator semantics, or tracing semantics as truth.

## 7. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/pydantic-ai-eval-report-evidence/README.md`
- `examples/pydantic-ai-eval-report-evidence/requirements.txt` only if the
  generator truly needs it
- `examples/pydantic-ai-eval-report-evidence/generate_synthetic_report.py` only
  if a clean local generator is viable
- `examples/pydantic-ai-eval-report-evidence/map_to_assay.py`
- `examples/pydantic-ai-eval-report-evidence/fixtures/valid.pydantic-ai.json`
- `examples/pydantic-ai-eval-report-evidence/fixtures/failure.pydantic-ai.json`
- `examples/pydantic-ai-eval-report-evidence/fixtures/malformed.pydantic-ai.json`
- `examples/pydantic-ai-eval-report-evidence/fixtures/valid.assay.ndjson`
- `examples/pydantic-ai-eval-report-evidence/fixtures/failure.assay.ndjson`

Fixture boundary note:

- v1 fixtures may omit `trace_ref` entirely
- v1 fixtures must not embed trace payloads
- v1 fixtures should keep report-wide analysis small enough to stay obviously
  artifact-first rather than dashboard-first

## 8. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 8.1 Preferred path

Preferred:

- a local generator that exercises the documented code-first evals path
- no hosted observability dependency
- no hidden credential requirement
- no runtime setup heavy enough to overshadow the sample

### 8.2 Hard fallback rule

If a real local generator would require:

- Logfire setup
- non-deterministic tracing/export behavior
- provider or telemetry setup heavy enough to turn the sample into an
  observability demo

then the sample must fall back to a **docs-backed frozen artifact shape**.

The sample must not become a half-working telemetry or eval-dashboard demo.

## 9. README boundary requirements

The eventual sample README must say:

- this is not a production Assay↔Pydantic AI adapter
- this does not freeze a new Assay Evidence Contract event type
- this does not treat evaluation scores, labels, assertions, or report outcomes
  as Assay truth
- this does not define a tracing or telemetry export contract

## 10. Outward channel strategy

If the sample lands and the surrounding outbound queue is quiet enough, the
first outward move should be **one small GitHub issue** in
`pydantic/pydantic-ai`.

Fallback channel for a softer exploratory question:

- `#pydantic-ai` in Pydantic Slack

The outward question should stay narrow:

> If an external evidence consumer wants the smallest honest Pydantic AI
> surface for bounded eval-result evidence, is a serialized
> `EvaluationReport`-style artifact roughly the right seam, or is there a
> thinner result surface you'd rather point them at?

## 11. Sequencing rule

This lane should not start implementation until the current wave is settled
enough that we are not opening too many active conversations at once.

That means:

- `mcp-agent` sample and Discussion must already be out
- current open threads should be allowed to sit without new nudges
- no outward Pydantic AI issue before the sample exists on `main`

## 12. Non-goals

- defining a tracing export contract for `pydantic-ai`
- building a Logfire or OpenTelemetry sink in this wave
- treating evaluation scores as model truth
- turning span-based evaluation into the first seam
- opening both GitHub and Slack outward channels at the same time

## References

- [pydantic/pydantic-ai](https://github.com/pydantic/pydantic-ai)
- [Pydantic Evals](https://ai.pydantic.dev/evals/)
- [Core Concepts](https://ai.pydantic.dev/evals/core-concepts/)
- [Debugging & Monitoring with Pydantic Logfire](https://ai.pydantic.dev/logfire/)
- [Getting Help](https://ai.pydantic.dev/help/)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
