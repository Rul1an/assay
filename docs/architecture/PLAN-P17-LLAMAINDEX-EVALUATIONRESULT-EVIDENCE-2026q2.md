# PLAN — P17 LlamaIndex EvaluationResult Evidence Interop (2026 Q2)

- **Date:** 2026-04-12
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next LlamaIndex interop lane after the
  current LiveKit, x402, Mastra, Langfuse, Browser Use, and Visa TAP wave.
  Include a small sample implementation, with no outward Discussion and no
  contract freeze yet.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has at least one natural maintainer or community channel for one
   small sample-backed boundary question.

`run-llama/llama_index` fits that pattern well enough to justify a formal
plan:

- the repo is large, active, and still moving quickly
- GitHub Discussions are enabled and visibly used for Q&A
- the evaluation stack exposes a small official result object through
  `EvaluationResult`
- that object is much narrower than traces, callback events, or broader
  observability layers

This is **not** a tracing plan.

This is **not** a callback export plan.

This is **not** a runtime correctness plan.

This is **not** a benchmark-suite plan.

This is a plan for a **bounded `EvaluationResult`-level evidence seam**.

## 2. Why LlamaIndex is a good `P17` candidate

LlamaIndex is the cleanest next same-space candidate after the current wave.

Why:

- it gives Assay another eval/result-first lane without collapsing back into
  tracing-first posture
- it offers a small documented result surface that already looks like external
  evidence
- the Discussions channel is a better fit for a small technical seam question
  than issue-first outreach
- the lane is socially easier than a platform-adjacent observability pitch

This makes LlamaIndex safer than jumping immediately to another
show-and-tell-heavy or platform-adjacent repo.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest LlamaIndex evaluation-result
> surface, not tracing exports, callback streams, observability sinks, or
> runtime correctness truth.

That means:

- LlamaIndex is the upstream evaluation context, not the truth source
- `EvaluationResult` is an observed evaluator output, not an Assay truth
  judgment
- Assay stays an external evidence consumer, not an authority on evaluator
  correctness or benchmark truth

Common anti-overclaim sentence:

> We are not asking Assay to inherit evaluator judgments or LlamaIndex
> evaluation semantics as truth.

## 4. Why not tracing-first

LlamaIndex has richer callback, trace, and workflow surfaces.

That would be the wrong first wedge.

Why:

- it would make the lane feel too similar to prior trace-heavy work
- it would widen the sample before proving that the smallest result surface is
  useful
- it would blur the line between "observed evaluation result" and
  "observability export"
- it would make outward framing read more like tooling integration than a
  bounded technical seam question

The cleaner first wedge is:

- one frozen serialized artifact derived from `EvaluationResult`
- one bounded pass/fail or score outcome
- one optional short textual feedback field
- no prompts
- no completions
- no traces
- no callback payloads

## 5. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented
`EvaluationResult` surface** as the first external-consumer seam.

The intent is to stay at the evaluation-result level:

- `passing`
- `score`
- `feedback`
- one short evaluator label if needed

Important framing rule:

> The sample uses a frozen artifact derived from `EvaluationResult`, not a
> claim that LlamaIndex already guarantees one fixed wire-export contract for
> external evidence consumers.

## 6. v1 artifact contract

### 6.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `timestamp`
- `evaluator_name`
- `outcome`
- `passing`

### 6.2 Optional fields

The first sample may include:

- `score`
- `feedback`
- `invalid_reason`
- `target_ref`

### 6.3 Important field boundaries

#### `passing`

This field is required because it is the smallest honest summary of the
evaluation result.

It must remain:

- observed evaluator output
- a bounded result field

It must not become:

- policy truth
- model quality truth
- application correctness truth

#### `score`

This field should remain small and scalar:

- one numeric score
- no score breakdown matrix
- no aggregate benchmark bundle

#### `feedback`

This field is optional in v1 and should stay bounded:

- one short evaluator comment
- no prompt transcript
- no answer transcript
- no reasoning dump

#### `invalid_reason`

If present, it must remain a short classifier or short bounded label:

- no stack trace
- no evaluator debug transcript
- no callback payload

## 7. Assay-side meaning

The sample may only claim bounded evaluation-result observation.

Assay must not treat as truth:

- evaluator correctness
- benchmark correctness
- model correctness
- task correctness beyond the observed result artifact

## 8. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/llamaindex-evalresult-evidence/README.md`
- `examples/llamaindex-evalresult-evidence/map_to_assay.py`
- `examples/llamaindex-evalresult-evidence/fixtures/valid.llamaindex.json`
- `examples/llamaindex-evalresult-evidence/fixtures/failure.llamaindex.json`
- `examples/llamaindex-evalresult-evidence/fixtures/malformed.llamaindex.json`
- `examples/llamaindex-evalresult-evidence/fixtures/valid.assay.ndjson`
- `examples/llamaindex-evalresult-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures should remain result-first
- v1 fixtures must not include traces, prompts, or completions
- v1 may omit optional fields entirely if the shape stays cleaner that way

## 9. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 9.1 Preferred path

Preferred:

- docs-backed frozen artifacts
- a tiny local generator only if it produces deterministic `EvaluationResult`
  values without provider credentials

### 9.2 Hard fallback rule

If a real generator would require:

- remote model providers
- credentials
- unstable evaluation setup
- a larger benchmark harness

then the sample should fall back to a **docs-backed frozen artifact shape**.

## 10. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 10.1 Valid

One valid evaluation result with:

- `passing=true`
- one bounded score
- optional short feedback

### 10.2 Failure

One failed evaluation result with:

- `passing=false`
- one bounded score if naturally present
- optional short feedback

### 10.3 Malformed

One malformed artifact that fails fast, for example:

- missing `passing`
- invalid `score`
- `feedback` not a string

## 11. Outward strategy

Do not open a LlamaIndex Discussion until the sample is on `main`.

After that:

- one GitHub Discussion
- category: Q&A
- one link
- one small boundary question
- no tracing pitch
- no callback pitch

Suggested outward question:

> If an external evidence consumer wants the smallest honest LlamaIndex
> surface, is an artifact derived from `EvaluationResult` roughly the right
> place to start, or is there an even thinner official result surface you
> would rather point them at?

## 12. Non-goals

This plan does not:

- define a tracing adapter
- define a callback export adapter
- define benchmark truth as Assay truth
- define runtime correctness as Assay truth

## References

- [LlamaIndex repo](https://github.com/run-llama/llama_index)
- [LlamaIndex discussions](https://github.com/run-llama/llama_index/discussions)
- [LlamaIndex evaluation usage pattern](https://docs.llamaindex.ai/en/v0.10.33/module_guides/evaluating/usage_pattern/)
- [LlamaIndex evaluation API reference](https://developers.llamaindex.ai/python/framework-api-reference/evaluation/)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
