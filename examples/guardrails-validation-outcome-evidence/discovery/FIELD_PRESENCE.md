# Field Presence Notes

Discovery run:

- date: 2026-04-25
- SDK: `guardrails-ai==0.10.0`
- language: Python
- public path: `Validator.validate(...)`
- source surface: direct `ValidationResult` objects (`PassResult` /
  `FailResult`)

## Raw returned result shape

The passing capture returned a `PassResult` with:

- `outcome = "pass"`
- `metadata = null`
- `validated_chunk = null`
- `value_override = null`

The failing capture returned a `FailResult` with:

- `outcome = "fail"`
- `error_message = "Value must include required term: approved"`
- `fix_value = "needs review approved"`
- `error_spans = null`
- `metadata = null`
- `validated_chunk = null`

## Reduction choices

The canonical fixture keeps:

- `validation_passed`, derived from the returned `outcome`
- `result.outcome`
- `result.error` for the failing result
- `validator_name` as the explicitly invoked validator identity

The canonical fixture omits:

- raw validation input value
- runtime validator metadata
- `validated_chunk`
- `value_override`
- `fix_value`
- `error_spans`
- any prompt, model, provider, reask, guard-history, or validator-log material

This sample uses the direct `ValidationResult` path because it is the smaller
public shape for P29. Guard-level `ValidationOutcome` remains discovery-relevant
for future work, but its raw and validated output fields are intentionally not
part of this first v1 lane.
