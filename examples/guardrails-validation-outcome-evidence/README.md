# Guardrails Validation Outcome Evidence Sample

This example turns one tiny frozen artifact derived from Guardrails AI's local
validation result path into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with one returned `PassResult` / `FailResult` from
  `Validator.validate(...)`
- keep one passing artifact, one failing artifact, and one malformed
  corrected-output case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep raw validation input, corrected/fixed output, metadata, reask payloads,
  guard history, and validator logs out of Assay truth

## What is in here

- `capture_probe.py`: runs one passing and one failing custom-validator
  capture through Guardrails' public `Validator.validate(...)` path
- `requirements.txt`: local probe dependency for the checked-in SDK path
- `discovery/valid.validation.inputs.json`: caller-side discovery inputs for
  the passing validation call
- `discovery/valid.returned.result.json`: raw returned `PassResult`
- `discovery/failure.validation.inputs.json`: caller-side discovery inputs for
  the failing validation call
- `discovery/failure.returned.result.json`: raw returned `FailResult`
- `discovery/FIELD_PRESENCE.md`: returned-field notes and reduction rationale
- `map_to_assay.py`: turns one reduced Guardrails artifact into an Assay-shaped
  placeholder envelope
- `fixtures/valid.guardrails.json`: one bounded passing artifact
- `fixtures/failure.guardrails.json`: one bounded failing artifact
- `fixtures/malformed.guardrails.json`: one malformed full/corrected-output
  import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this surface

Guardrails has a broader validation surface than this sample uses:

- guard-level `ValidationOutcome`
- raw LLM output
- validated or corrected output
- reask payloads
- validator logs and history
- validator metadata
- hub validators and remote services
- streaming validation

This sample starts on the smaller direct validation-result surface:

- one local validator call
- one returned `ValidationResult`
- one bounded result bag with pass/fail and a short failure message

That keeps the first wedge smaller than:

- Guardrails orchestration truth
- raw-output truth
- corrected-output truth
- prompt/model/provider truth
- guard-history or validator-log import

## Live discovery note

This sample is grounded in a small local probe run on **2026-04-25** against
`guardrails-ai==0.10.0`.

The passing direct-validator result returned:

- `outcome = "pass"`
- no failure message
- no meaningful override, metadata, or validated chunk

The failing direct-validator result returned:

- `outcome = "fail"`
- short `error_message`
- `fix_value`
- no error spans, metadata, or validated chunk

For the reduced artifact:

- `validation_passed` is derived from the returned `outcome`
- `result.outcome` is copied from the returned result
- `result.error` preserves the short failure message
- `validator_name` records the explicitly invoked local validator
- `fix_value`, `value_override`, and raw validation input stay discovery-only

## Re-run the local discovery probe

```bash
python3.12 -m venv /tmp/p29-guardrails-venv
/tmp/p29-guardrails-venv/bin/python -m pip install --upgrade pip
/tmp/p29-guardrails-venv/bin/python -m pip install \
  -r examples/guardrails-validation-outcome-evidence/requirements.txt
/tmp/p29-guardrails-venv/bin/python \
  examples/guardrails-validation-outcome-evidence/capture_probe.py
```

The probe prints the observed package version and writes raw input plus returned
artifacts into `discovery/`.

## Map the checked-in passing artifact

```bash
python3 examples/guardrails-validation-outcome-evidence/map_to_assay.py \
  examples/guardrails-validation-outcome-evidence/fixtures/valid.guardrails.json \
  --output examples/guardrails-validation-outcome-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-25T13:00:00Z \
  --overwrite
```

## Map the checked-in failing artifact

```bash
python3 examples/guardrails-validation-outcome-evidence/map_to_assay.py \
  examples/guardrails-validation-outcome-evidence/fixtures/failure.guardrails.json \
  --output examples/guardrails-validation-outcome-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-25T13:01:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/guardrails-validation-outcome-evidence/map_to_assay.py \
  examples/guardrails-validation-outcome-evidence/fixtures/malformed.guardrails.json \
  --output /tmp/guardrails-malformed.assay.ndjson \
  --import-time 2026-04-25T13:02:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry raw output, corrected output, fix values, metadata, and validator logs
into a one-result v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Guardrails validation as application safety truth
- import raw validation input, prompt text, or raw LLM output
- import validated, fixed, corrected, or coerced output
- import `fix_value`, `value_override`, or `validated_chunk`
- import validator metadata, error spans, logs, reask payloads, or guard history
- model streaming validation
- model Guardrails hub coverage or remote service behavior
- partially import larger Guardrails `ValidationOutcome` or history envelopes
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest Guardrails validation-result surface,
not broad Guardrails support.
