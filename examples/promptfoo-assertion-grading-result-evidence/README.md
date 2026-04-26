# Promptfoo Assertion GradingResult Evidence Sample

This example turns one tiny artifact derived from Promptfoo's deterministic
`equals` assertion result into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with one extracted assertion-level result from a Promptfoo JSONL row
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep raw prompt/output/expected/config, provider responses, and full
  Promptfoo eval wrappers out of Assay truth

## What is in here

- `capture_probe.mjs`: runs one positive and one negative Promptfoo `equals`
  assertion and saves raw discovery artifacts
- `package.json`: local probe dependency for the checked-in Promptfoo path
- `discovery/promptfoo.inputs.json`: the exact emitted outputs and assertion
  config used for discovery
- `discovery/valid.full-jsonl-row.json`: the raw successful Promptfoo JSONL
  row
- `discovery/failure.full-jsonl-row.json`: the raw failed Promptfoo JSONL row
- `discovery/valid.surfaced.assertion-result.json`: the extracted successful
  assertion component
- `discovery/failure.surfaced.assertion-result.json`: the extracted failed
  assertion component
- `discovery/FIELD_PRESENCE.md`: emitted-vs-wrapper-vs-surfaced notes and
  reduction rationale
- `map_to_assay.py`: turns one reduced Promptfoo assertion result artifact into
  an Assay-shaped placeholder envelope
- `fixtures/valid.promptfoo.json`: one bounded positive artifact derived from
  the surfaced assertion component
- `fixtures/failure.promptfoo.json`: one bounded negative artifact derived from
  the surfaced assertion component
- `fixtures/malformed.promptfoo.json`: one malformed wrapper-plus-payload import
  case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this surface

Promptfoo has a much broader public surface than this sample uses:

- full JSON, JSONL, YAML, XML, and HTML eval exports
- prompt/provider comparison matrices
- red-team reports
- model-graded assertions
- provider responses, cost, latency, and token usage
- config, vars, prompt, output, and expected payloads

This sample starts on the smaller deterministic assertion surface:

- one `equals` assertion
- one extracted assertion component result
- one bounded result bag

The key discovery detail is that the full JSONL row is too broad. The row-level
`gradingResult` is also an aggregate over the row. For the first P28 lane, the
honest small seam is the single assertion component at
`gradingResult.componentResults[0]`.

## Live discovery note

This sample is grounded in a small local probe run on 2026-04-24 against
`promptfoo@0.119.0`.

The important boundary is now clear:

- emitted model outputs and assertion config are not the surfaced result
- the full JSONL row is a wrapper and carries provider/test/run context
- the row-level `gradingResult` is aggregate row outcome
- the single assertion component carries `pass`, `score`, `reason`, and
  adjacent assertion config
- only `assertion.type`, `pass`, and integer `score` are needed for the v1
  canonical artifact
- short `reason` is optional and may be dropped

The failed assertion reason includes the raw compared values:

```text
Expected output "Goodbye world" to equal "Hello world"
```

That is useful upstream explanation, but it would smuggle raw output/expected
payloads into the v1 fixture. The failure fixture therefore omits `reason`.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded negative
assertion artifact, not an infrastructure failure.

## Re-run the local discovery probe

```bash
cd examples/promptfoo-assertion-grading-result-evidence
npm ci
npm run capture
```

The probe prints the observed package version and writes raw input, full JSONL
row, and extracted assertion result artifacts into `discovery/`.

## Map the checked-in valid artifact

```bash
python3 examples/promptfoo-assertion-grading-result-evidence/map_to_assay.py \
  examples/promptfoo-assertion-grading-result-evidence/fixtures/valid.promptfoo.json \
  --output examples/promptfoo-assertion-grading-result-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-24T12:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/promptfoo-assertion-grading-result-evidence/map_to_assay.py \
  examples/promptfoo-assertion-grading-result-evidence/fixtures/failure.promptfoo.json \
  --output examples/promptfoo-assertion-grading-result-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-24T12:01:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/promptfoo-assertion-grading-result-evidence/map_to_assay.py \
  examples/promptfoo-assertion-grading-result-evidence/fixtures/malformed.promptfoo.json \
  --output /tmp/promptfoo-malformed.assay.ndjson \
  --import-time 2026-04-24T12:02:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry raw compared values, component wrappers, and a boolean score into a
single-assertion v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Promptfoo eval-run semantics as Assay truth
- import raw prompt, output, expected, vars, or assertion value
- import full JSON/JSONL/YAML/XML export wrappers
- import provider responses, cost, latency, token usage, or stats
- partially import larger Promptfoo result envelopes
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest Promptfoo deterministic assertion
result surface, not a broad Promptfoo eval-run lane.
