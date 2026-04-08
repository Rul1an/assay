# PLAN — P10 Agno Accuracy Eval Evidence Interop (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next Agno interop lane after the current
  framework, protocol, runtime-accounting, and eval-report wave. No sample
  implementation, no outward post, no contract freeze in this slice.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as truth,
3. the repo has a natural maintainer channel for one small sample-backed question.

`agno-agi/agno` currently fits that pattern well:

- the repo is large, active, and visibly growing
- Discussions are enabled, with active `Q&A`, `Ideas`, and `Show and tell`
- the docs separate Evals from Tracing clearly enough that we can choose one
  seam instead of mixing both

That makes Agno a strong next candidate, but only if Assay keeps the first
slice on a small eval-result artifact and does not drift into another
trace-first or observability-first pitch.

This is **not** a trace-export plan.

This is **not** an AgentOS platform-export plan.

This is a plan for a **bounded eval-result seam derived from Agno accuracy
evals**.

## 2. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest eval-result surface exposed by Agno,
> not a tracing export, AgentOS platform API, or runtime truth surface.

That means:

- Agno is the upstream context, not the truth source
- `AccuracyEval` / `AccuracyResult` are eval-result surfaces, not trace surfaces
- Assay stays an external evidence consumer, not a judge of evaluator
  correctness, runtime correctness, or tracing correctness

## 3. Why not trace-first or Agent-as-Judge first

Agno publicly documents both Tracing and Evals, but those surfaces do not make
equally good first seams.

Tracing is documented as the broader OpenTelemetry-style observability layer.
Choosing it first would make this lane look too similar to:

- Microsoft Agent Framework trace export
- OpenAI Agents `TraceProcessor`
- LangGraph task stream

`AgentAsJudgeEval` is also not the best first seam. It is legitimate, but more
semantically loaded than the basic accuracy path because it centers evaluator
judgment configuration from the start.

The cleaner first wedge is:

- one artifact derived from the documented `AccuracyEval` / `AccuracyResult`
  path
- bounded scores
- bounded average score
- minimal optional references only if the chosen sample shape needs them

This keeps the lane clearly different from trace-first lanes while still
staying anchored in an official Agno surface.

## 4. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented
`AccuracyEval` / `AccuracyResult` surface** as the first external-consumer seam.

This seam is:

- eval-first
- reviewable
- smaller than tracing
- smaller than AgentOS eval-run APIs
- lighter than `AgentAsJudgeEval`
- directly aligned with the public Accuracy docs

This is intentionally not:

- tracing export
- OpenTelemetry export
- AgentOS `/eval-runs` API export
- `AgentAsJudgeEval` as the first seam
- performance or reliability evals as the first seam

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented
> `AccuracyEval` / `AccuracyResult` surface, not a claim that Agno already
> guarantees a fixed wire-export contract.

## 5. v1 artifact contract

### 5.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `eval_type`
- `eval_name`
- `timestamp`
- `outcome`
- `num_iterations`
- `scores`
- `avg_score`

### 5.2 Optional fields

The first sample may include:

- `threshold`
- `input_label`
- `expected_output_ref`
- `guidelines_ref`
- `agent_ref`

### 5.3 Important field boundaries

#### `scores`

This field is required in the frozen sample shape.

It should stay small and bounded:

- integer-valued scores in v1
- no raw evaluator reasoning payload
- no full prompt or output bodies

This requirement belongs to the sample shape, not to an upstream claim that
Agno guarantees a universal serialized `scores` contract.

#### `avg_score`

This field is required in the frozen sample shape but remains upstream eval
semantics only.

It must not be promoted into:

- quality truth
- evaluator truth
- runtime truth

#### `threshold`

This field is optional in v1.

It should only appear if the chosen frozen sample shape carries it explicitly.
Its presence must not imply that Agno already guarantees a fixed serialized
export contract for that field.

#### References

The optional reference fields must stay bounded:

- small label
- opaque id
- short reference string

Not allowed in v1:

- full expected output payload
- full guidelines payload
- full agent config
- trace payload

## 6. Assay-side meaning

The sample may only claim bounded eval-result observation.

Assay must not treat as truth:

- evaluator correctness
- runtime correctness
- pass/fail semantics beyond the observed upstream artifact
- trace correctness
- AgentOS platform state

Common anti-overclaim sentence:

> We are not asking Assay to inherit Agno eval judgments, evaluator semantics,
> runtime semantics, or tracing semantics as truth.

## 7. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/agno-accuracy-evidence/README.md`
- `examples/agno-accuracy-evidence/requirements.txt` only if the generator
  truly needs it
- `examples/agno-accuracy-evidence/generate_synthetic_result.py` only if a
  clean local generator is viable
- `examples/agno-accuracy-evidence/map_to_assay.py`
- `examples/agno-accuracy-evidence/fixtures/valid.agno.json`
- `examples/agno-accuracy-evidence/fixtures/failure.agno.json`
- `examples/agno-accuracy-evidence/fixtures/malformed.agno.json`
- `examples/agno-accuracy-evidence/fixtures/valid.assay.ndjson`
- `examples/agno-accuracy-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional reference field
- v1 fixtures must not embed trace payloads
- v1 fixtures should keep the export shape obviously artifact-first rather than
  dashboard-first or platform-first

## 8. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 8.1 Preferred path

Preferred:

- a local generator that exercises the documented `AccuracyEval` flow
- no tracing dependency
- no AgentOS dependency
- no hidden credential requirement
- no runtime setup heavy enough to overshadow the sample

### 8.2 Hard fallback rule

If a real local generator would require:

- provider credentials
- non-deterministic remote evaluation behavior
- model/runtime setup heavy enough to turn the sample into a hosted eval demo

then the sample must fall back to a **docs-backed frozen artifact shape**.

The sample must not become a half-working hosted eval demo.

## 9. README boundary requirements

The eventual sample README must say:

- this is not a production Assay↔Agno adapter
- this does not freeze a new Assay Evidence Contract event type
- this does not treat scores, average score, or outcomes as Assay truth
- this does not turn tracing into the first seam
- this does not claim a fixed upstream wire-export contract

## 10. Outward channel strategy

If the sample lands and the surrounding outbound queue is quiet enough, the
first outward move should be **one small Discussion** in `agno-agi/agno`.

Best-fit category candidate:

- `Q&A`

Why `Q&A` instead of `Show and tell`:

- the question is about the smallest honest seam
- the repo already uses `Q&A` for focused technical boundary questions
- `Show and tell` is more likely to read as project showcase or promotion

The outward question should stay narrow:

> If an external evidence consumer wants the smallest honest Agno eval-result
> surface, is an artifact derived from `AccuracyEval` / `AccuracyResult`
> roughly the right first seam, or is there a thinner result surface you'd
> rather point them at?

## 11. Sequencing rule

This lane should not begin outward outreach until the newest lanes have had
time to breathe.

That means:

- the `pydantic-ai` sample and issue should already be out
- the `mcp-agent` discussion should be allowed to sit without another nudge
- no LangGraph retry or UCP outward move should happen at the same time

Implementation planning can start now, but outward Agno posting should still
follow the one-lane-at-a-time discipline.

## 12. Non-goals

- building another trace-first sample
- opening on `AgentAsJudgeEval` instead of the simpler accuracy path
- consuming AgentOS eval-run APIs as the first seam
- importing evaluator truth or runtime truth
- treating `threshold` as a guaranteed upstream serialized field

## References

- [agno-agi/agno](https://github.com/agno-agi/agno)
- [Agno docs — Evals overview](https://docs.agno.com/features/evals/overview)
- [Agno docs — Accuracy Evals](https://docs.agno.com/evals/accuracy/overview)
- [Agno docs — Agent as Judge Evals](https://docs.agno.com/evals/agent-as-judge/overview)
- [Agno docs — Tracing overview](https://docs.agno.com/tracing/overview)
- [Agno Discussions](https://github.com/agno-agi/agno/discussions)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
