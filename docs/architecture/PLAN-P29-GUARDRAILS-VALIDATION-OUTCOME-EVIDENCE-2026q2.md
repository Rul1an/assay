# PLAN — P29 Guardrails Validation Outcome Evidence

- **Date:** 2026-04-25
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Explore one bounded Guardrails AI-adjacent
  evidence lane built around a single validation outcome from a local
  Guardrails validation call. This plan is outcome-first and correction-agnostic:
  it does not import prompts, raw LLM output, corrected output, reask messages,
  validator orchestration history, or Guardrails platform/runtime truth.

## 1. Why this plan exists

Guardrails AI is a strong P29 candidate because it opens a safety / validation
family rather than another evaluator-return family.

Guardrails documents two relevant result surfaces:

- `ValidationOutcome`, the final output from a Guard execution
- `ValidationResult` / `PassResult` / `FailResult`, the lower-level validator
  result shape

Those public surfaces make P29 plausible, but they also make the boundary easy
to overrun. A `ValidationOutcome` can include raw LLM output, validated output,
reask data, and errors. Those fields are useful for Guardrails users, but they
are too rich for the first Assay artifact.

P29 should therefore stay outcome-first:

- did validation pass
- what bounded failure/error signal was surfaced
- what local validation call produced the outcome

It should stay correction-agnostic:

- do not treat corrected or coerced output as truth
- do not import raw output as evidence payload
- do not model the full guard history

## 2. What this plan is and is not

This plan is for:

- one local Guardrails validation call
- one returned validation outcome / result object
- one bounded pass/fail outcome artifact
- one discovery pass over the public returned shape
- one upstream docs/repro contribution only if a concrete shape ambiguity is
  found

This plan is not for:

- full Guardrails support
- prompt validation truth
- raw LLM output truth
- corrected or fixed output truth
- validator hub coverage
- reask orchestration
- streaming validation
- full guard history
- exception behavior across all `on_fail` modes
- platform or observability truth

## 3. Hard positioning rule

P29 v1 claims only one bounded Guardrails validation outcome as imported safety
signal evidence. It does not claim the raw output is true, the corrected output
is true, the validator is sufficient, the guard is complete, or the application
is safe.

That means:

- Guardrails remains the source of the observed validation outcome
- Assay imports only the smallest honest outcome fields
- Assay does not inherit correction, reask, prompt, or runtime semantics as
  truth

## 4. Recommended surface

The first P29 surface should stay on exactly one move:

- run one public local Guardrails validation path
- capture the returned `ValidationOutcome` or direct validator
  `ValidationResult`
- reduce exactly one pass/fail outcome signal

The preferred first hypothesis is high-level `ValidationOutcome`, because it is
documented as the final output from a Guard execution.

Discovery must stay honest, though. If the high-level outcome surface is too
output-heavy for a small artifact, the implementation may reduce only its
bounded outcome fields while preserving the raw outcome separately as discovery
evidence. It must not silently switch to full output import.

Not:

- raw prompt text
- raw LLM output
- validated/corrected output
- `fix_value` as truth
- `value_override` as truth
- reask prompts
- validator logs
- full guard history
- streaming chunks
- platform traces

## 5. Canonical v1 artifact thesis

The v1 artifact must be frozen from a captured returned Guardrails outcome /
result object, not from docs-only assumptions.

Illustrative v1 shape:

```json
{
  "schema": "guardrails.validation-outcome.export.v1",
  "framework": "guardrails-ai",
  "surface": "validation_outcome",
  "target_kind": "validation_call",
  "validation_passed": false,
  "result": {
    "outcome": "fail",
    "error": "Value failed validation."
  }
}
```

Optional reviewer support, only if naturally present and bounded:

- `call_id_ref`
- `validator_name`
- `result.error`

Not allowed in v1:

- raw LLM output
- validated output
- corrected output
- `fix_value`
- `value_override`
- reask payloads
- full guard history
- validator logs
- inline metadata bags
- prompt text
- model/provider metadata
- streaming chunks

## 6. Field boundaries

### 6.1 `target_kind`

For v1, the allowed value is:

- `validation_call`

This names the evaluation level only. It does not imply that Assay carries
stable prompt identity, raw output identity, validator identity, or application
task identity.

### 6.2 `validation_passed`

`validation_passed` is the top-level validation outcome when naturally present.

It must not become:

- application safety truth
- model behavior truth
- corrected-output truth
- policy completeness truth

### 6.3 `result.outcome`

`result.outcome` is the bounded pass/fail label.

Allowed values for v1:

- `pass`
- `fail`

If discovery shows the first returned shape uses only `validation_passed` and
does not carry an outcome string, the reducer may derive `pass` / `fail` from
that boolean, but it must document that as a reduction choice.

### 6.4 `result.error`

`result.error` is optional and must remain short.

It may preserve a bounded validation failure message. It must not preserve:

- full validator reasoning
- raw invalid text
- prompt excerpts
- stack traces
- multi-line remediation plans

### 6.5 `call_id_ref`

`call_id_ref` may be included only if naturally present on the returned
`ValidationOutcome`.

It is a reviewer anchor, not a claim that Assay imports Guardrails call history.

### 6.6 Corrected / coerced output

Corrected, fixed, coerced, or validated output is discovery context only.

Even when Guardrails returns `validated_output`, `fix_value`, or
`value_override`, v1 must not import it into the canonical artifact. That rule is
the core of P29.

## 7. Observed vs derived rule

Capture separately:

- raw returned outcome / result object
- emitted validation input needed for reproduction
- package version and local runtime notes

The canonical v1 artifact must never include:

- prompt text
- raw output
- validated output
- corrected output
- metadata used by validators
- synthetic hashes of omitted raw values

## 8. Cardinality rule

V1 is single-outcome only.

Malformed for v1:

- arrays of validation outcomes
- full guard history
- validator log lists
- streaming validation chunk sequences
- reask histories
- partial import of "the first failed validator" from a larger envelope without
  an explicit extracted single-outcome discovery artifact

## 9. Discovery gate

Do not freeze fixtures until discovery captures:

- one passing returned validation outcome / result
- one failing returned validation outcome / result
- raw emitted inputs stored separately from raw returned outputs
- explicit note on whether the source was `ValidationOutcome` or direct
  `ValidationResult`
- explicit note on whether `validated_output`, `fix_value`, `value_override`,
  or reask data was present and intentionally excluded

If Guard-level `ValidationOutcome` and direct-validator `ValidationResult`
produce materially different bounded shapes, freeze the lane to one public path
first. Do not merge them into one pretend contract.

## 10. Upstream contribution posture

P29 should not open with an abstract API-stability question.

The useful upstream move is docs/repro clarity around the returned validation
shape:

> I was tightening a small local validation-result example and noticed this
> part of the returned outcome was easy to misread. This patch makes the
> pass/fail and corrected-output boundary clearer for readers.

Only contribute upstream if there is a concrete docs or example gap around:

- `ValidationOutcome` fields
- `ValidationResult` / `PassResult` / `FailResult`
- `validation_passed`
- error behavior
- corrected / fixed output fields

Do not mention Assay unless maintainers ask what prompted the clarification.

## 11. Concrete repo deliverable

After this plan, the implementation PR should add:

- `examples/guardrails-validation-outcome-evidence/README.md`
- a small local capture probe using one public Guardrails validation path
- raw discovery artifacts for emitted input and returned outcome/result
- one reduced valid fixture
- one reduced failure fixture
- one malformed fixture for corrected-output or full-history imports
- a mapper into the existing placeholder NDJSON pattern
- an `examples/README.md` index entry

## 12. Non-goals

P29 does not:

- implement Guardrails orchestration
- validate prompts or raw LLM output as Assay truth
- import corrected output
- import reask flow
- model streaming validation
- model validator hub coverage
- model Guardrails logs or call history
- model model/provider behavior

## References

- Guardrails AI API Reference — Guards / `ValidationOutcome`:
  https://guardrailsai.com/docs/api_reference_markdown/guards
- Guardrails AI API Reference — Validators / `ValidationResult`:
  https://guardrailsai.com/guardrails/docs/api_reference_markdown/validator
- Guardrails AI Concepts — Validator OnFail Actions:
  https://guardrailsai.com/guardrails/docs/concepts/validator_on_fail_actions
