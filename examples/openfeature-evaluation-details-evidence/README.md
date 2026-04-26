# OpenFeature EvaluationDetails Evidence Sample

This example turns one tiny frozen artifact derived from OpenFeature's
detailed flag evaluation API into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start with one returned `EvaluationDetails` object from a public SDK call
- keep one normal artifact, one fallback/error artifact, and one malformed
  provider-state case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep provider config, targeting rules, evaluation context, hooks, telemetry,
  rollout state, and application correctness out of Assay truth

## What is in here

- `capture_probe.py`: runs one normal and one fallback `get_boolean_details`
  capture using OpenFeature's in-memory provider
- `requirements.txt`: local probe dependency for the checked-in SDK path
- `discovery/valid.evaluation.inputs.json`: caller-side discovery inputs for
  the normal flag resolution
- `discovery/valid.returned.details.json`: raw returned `EvaluationDetails`
  for the normal flag resolution
- `discovery/fallback.evaluation.inputs.json`: caller-side discovery inputs for
  the missing-flag fallback
- `discovery/fallback.returned.details.json`: raw returned `EvaluationDetails`
  for the missing-flag fallback
- `discovery/FIELD_PRESENCE.md`: returned-field notes and reduction rationale
- `map_to_assay.py`: turns one reduced OpenFeature artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.openfeature.json`: one bounded normal artifact
- `fixtures/fallback.openfeature.json`: one bounded fallback/error artifact
- `fixtures/malformed.openfeature.json`: one malformed provider/config import
  case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/fallback.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this surface

OpenFeature has a broader surface than this sample uses:

- providers
- flag definitions
- targeting rules
- rollout and variant configuration
- hooks
- evaluation context
- telemetry and observability mappings
- OFREP and provider-specific protocols

This sample starts on the smaller decision-detail surface:

- one detailed boolean evaluation call
- one returned `EvaluationDetails` object
- one bounded result bag with value, variant/reason where present, and error
  fields for the fallback case

That keeps the first wedge smaller than:

- provider configuration truth
- application feature correctness
- targeting or rollout truth
- provider metadata import
- OpenTelemetry flag-evaluation telemetry

## Live discovery note

This sample is grounded in a small local probe run on **2026-04-25** against
`openfeature-sdk==0.8.4`.

The normal in-memory provider result returned:

- `flag_key`
- boolean `value`
- `variant`
- `reason`
- empty `flag_metadata`
- no error fields

The missing-flag fallback returned:

- `flag_key`
- default boolean `value`
- `reason = "ERROR"`
- `error_code = "FLAG_NOT_FOUND"`
- short `error_message`
- empty `flag_metadata`

For the reduced artifact:

- `flag_key` is the natural evaluated-target anchor
- `result.value` is copied from the returned detail object
- `result.variant` and `result.reason` are included only when non-empty
- fallback `error_code` / `error_message` stay in `result`
- empty `flag_metadata` is intentionally omitted
- provider config, defined flags, default values, and evaluation context stay
  discovery-only

The repo corpus uses `fallback` naming for the missing-flag case because this
is still a valid bounded OpenFeature evaluation-detail artifact. It is not an
infrastructure failure.

## Re-run the local discovery probe

```bash
python3.12 -m venv /tmp/p30-openfeature-venv
/tmp/p30-openfeature-venv/bin/python -m pip install --upgrade pip
/tmp/p30-openfeature-venv/bin/python -m pip install \
  -r examples/openfeature-evaluation-details-evidence/requirements.txt
/tmp/p30-openfeature-venv/bin/python \
  examples/openfeature-evaluation-details-evidence/capture_probe.py
```

The probe prints the observed package version and writes raw input plus returned
artifacts into `discovery/`.

## Map the checked-in valid artifact

```bash
python3 examples/openfeature-evaluation-details-evidence/map_to_assay.py \
  examples/openfeature-evaluation-details-evidence/fixtures/valid.openfeature.json \
  --output examples/openfeature-evaluation-details-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-25T12:00:00Z \
  --overwrite
```

## Map the checked-in fallback artifact

```bash
python3 examples/openfeature-evaluation-details-evidence/map_to_assay.py \
  examples/openfeature-evaluation-details-evidence/fixtures/fallback.openfeature.json \
  --output examples/openfeature-evaluation-details-evidence/fixtures/fallback.assay.ndjson \
  --import-time 2026-04-25T12:01:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/openfeature-evaluation-details-evidence/map_to_assay.py \
  examples/openfeature-evaluation-details-evidence/fixtures/malformed.openfeature.json \
  --output /tmp/openfeature-malformed.assay.ndjson \
  --import-time 2026-04-25T12:02:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry provider configuration, evaluation context, inline flag metadata, and
caller-side defaults into a one-detail v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat OpenFeature or provider resolution semantics as Assay truth
- import provider configuration, targeting rules, rollout state, or flag
  definitions
- import evaluation context, targeting keys, or transaction context
- import inline flag metadata by default
- import OpenFeature telemetry or hook state
- claim application correctness after a flag evaluation
- partially import larger provider, OFREP, or telemetry envelopes
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest OpenFeature `EvaluationDetails`
surface, not broad OpenFeature or feature-management support.
