# Browser Use History Evidence Sample

This example turns one tiny frozen artifact derived from the documented Browser
Use `AgentHistoryList` result surface into bounded, reviewable external
evidence for Assay.

It is intentionally small:

- start with one frozen run-result artifact shape derived from documented
  selectors such as `action_names()`, `final_result()`, and `errors()`
- keep the sample to one valid artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep `action_history`, `final_result`, and `errors` as observed Browser Use
  run-result data only
- keep screenshots, DOM/HTML dumps, structured output payloads, telemetry, and
  observability semantics out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Browser Use history artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.browser-use.json`: one completed Browser Use run artifact
- `fixtures/failure.browser-use.json`: one failed Browser Use run artifact
- `fixtures/malformed.browser-use.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed import time

## Why this seam

This sample treats a frozen serialized artifact derived from the documented
`AgentHistoryList` surface as the current best first seam hypothesis for
Browser Use.

That keeps the first slice on local run history and output only. It does not
turn the sample into:

- a Laminar export lane
- an OpenLIT export lane
- a telemetry lane
- a screenshot or DOM dump lane
- a generalized browser automation demo

The checked-in fixtures are deliberately docs-backed and frozen. Browser Use is
an actual browser automation stack with runtime setup and optional cloud
surfaces, so this sample keeps the evidence seam honest without pretending that
the smallest stable path is already a full live generator in this repo.

The checked-in fixtures also omit every optional reference on purpose. Those
fields may exist later in a bounded sample shape, but v1 keeps the seam on
small action-history reductions, a short final result representation, and
bounded error observations only.

## Map the checked-in valid artifact

```bash
python3 examples/browser-use-history-evidence/map_to_assay.py \
  examples/browser-use-history-evidence/fixtures/valid.browser-use.json \
  --output examples/browser-use-history-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-08T12:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/browser-use-history-evidence/map_to_assay.py \
  examples/browser-use-history-evidence/fixtures/failure.browser-use.json \
  --output examples/browser-use-history-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-08T12:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/browser-use-history-evidence/map_to_assay.py \
  examples/browser-use-history-evidence/fixtures/malformed.browser-use.json \
  --output /tmp/browser-use-malformed.assay.ndjson \
  --import-time 2026-04-08T12:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Browser Use action history, output semantics, browser-state semantics,
  or observability semantics as Assay truth
- imply that Assay independently verified browser automation correctness,
  workflow correctness, or page correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest local run-result surface exposed by
Browser Use, not an observability export, cloud trace, telemetry stream, or
MCP protocol record.

We are not asking Assay to inherit Browser Use action history, output
semantics, browser-state semantics, or observability semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the
placeholder envelopes are honest about deterministic hashing without pretending
to be a full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.browser-use.json`: bounded valid run-history artifact
- `fixtures/failure.browser-use.json`: bounded failure run-history artifact
- `fixtures/malformed.browser-use.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
