# LangWatch Custom Span Evaluation Evidence Sample

This example turns one tiny frozen artifact derived from LangWatch's custom
span evaluation path into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with one reduced artifact derived from one surfaced child
  `evaluation` span
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep broader trace, dataset, evaluation-session, and platform workflow truth
  out of Assay truth

## What is in here

- `capture_probe.py`: small local LangWatch probe that emits one positive and
  one negative `add_evaluation(...)` call and saves raw discovery payloads
- `requirements.txt`: local probe dependencies for the checked-in SDK path
- `discovery/valid.emitted.input.json`: the public emitted input we sent for
  the valid live evaluation
- `discovery/valid.surfaced.trace.response.json`: the wider surfaced trace
  response for the valid live evaluation
- `discovery/valid.surfaced.evaluation.span.json`: the extracted surfaced child
  `evaluation` span for the valid live evaluation
- `discovery/failure.emitted.input.json`: the public emitted input we sent for
  the failure live evaluation
- `discovery/failure.surfaced.trace.response.json`: the wider surfaced trace
  response for the failure live evaluation
- `discovery/failure.surfaced.evaluation.span.json`: the extracted surfaced
  child `evaluation` span for the failure live evaluation
- `discovery/FIELD_PRESENCE.md`: emitted-vs-surfaced notes and reduction
  rationale
- `map_to_assay.py`: turns one reduced LangWatch custom span evaluation
  artifact into an Assay-shaped placeholder envelope
- `fixtures/valid.langwatch.json`: one bounded positive artifact derived from
  the surfaced child evaluation span
- `fixtures/failure.langwatch.json`: one bounded negative artifact derived from
  the surfaced child evaluation span
- `fixtures/malformed.langwatch.json`: one malformed trace-envelope import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time

## Why this seam

LangWatch has a much broader public surface than this sample uses:

- traces
- offline evaluation runs
- datasets
- scenarios
- prompt and workflow tooling

This sample intentionally does **not** start there.

It starts on the smaller public custom evaluation seam:

- one `add_evaluation(...)` call
- one evaluated span
- one bounded result bag

That keeps the first wedge smaller than:

- full trace exports
- evaluation-session wrappers
- dataset row truth
- prompt-management state
- broader LangWatch platform truth

## Live discovery note

This sample is stronger than docs-only.

It is grounded in a small live local probe against LangWatch on **2026-04-22**:

- one local LangWatch docker stack was started at `http://127.0.0.1:5560`
- one seeded project API key was used for capture
- one positive custom span evaluation was emitted and surfaced
- one negative custom span evaluation was emitted and surfaced

The important boundary is now clear:

- emitted `add_evaluation(...)` input is not the same thing as surfaced public
  shape
- the first public readback came through a trace response
- inside that trace response, the useful bounded unit was the child
  `evaluation` span
- the top-level `evaluations` helper array was not stable enough across the two
  live captures to serve as the seam

That is why the reduced artifact is derived from the surfaced child
`evaluation` span rather than from:

- emitted input alone
- the wider trace envelope
- the top-level `evaluations` helper array

For the reduced artifact:

- `entity_id_ref` is taken from the surfaced child span's `parent_id`
- `evaluation_name` is taken from the surfaced child span's `name`
- `result` is reduced from `output.value`
- `timestamp` is taken from the surfaced child span's `timestamps.finished_at`
- `trace_id_ref` is included only because it appeared naturally on the surfaced
  child span
- raw `span_id`, raw `output`, raw `params`, raw `status`, and wider trace
  envelope fields stay out of the canonical artifact

One more operational note surfaced during capture:

- the current public Python SDK path on `langwatch==0.22.0` needed `pandas`,
  `tenacity`, and `tqdm` installed alongside the base SDK for
  `add_evaluation(...)` to run cleanly

That is a capture-environment detail, not part of the reduced evidence seam.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded negative
evaluation artifact, not an infrastructure failure.

## Re-run the local discovery probe

Start a local LangWatch stack first:

```bash
gh repo clone langwatch/langwatch /tmp/langwatch-p25 -- --depth=1
cp /tmp/langwatch-p25/langwatch/.env.example /tmp/langwatch-p25/langwatch/.env
docker compose -f /tmp/langwatch-p25/compose.yml up -d
docker compose -f /tmp/langwatch-p25/compose.yml exec -T \
  -e LANGWATCH_API_KEY=sk-lw-p25-local-test-key \
  app sh -lc 'cd /app/langwatch && pnpm run prisma:seed'
```

Then run:

```bash
python3.12 -m venv /tmp/p25-langwatch-venv
/tmp/p25-langwatch-venv/bin/python -m pip install --upgrade pip
/tmp/p25-langwatch-venv/bin/python -m pip install \
  -r examples/langwatch-custom-span-evaluation-evidence/requirements.txt
LANGWATCH_API_KEY=sk-lw-p25-local-test-key \
LANGWATCH_ENDPOINT=http://127.0.0.1:5560 \
/tmp/p25-langwatch-venv/bin/python \
  examples/langwatch-custom-span-evaluation-evidence/capture_probe.py
```

The probe prints the live trace ids and writes raw emitted plus surfaced
artifacts into `discovery/`.

## Map the checked-in valid artifact

```bash
python3 examples/langwatch-custom-span-evaluation-evidence/map_to_assay.py \
  examples/langwatch-custom-span-evaluation-evidence/fixtures/valid.langwatch.json \
  --output examples/langwatch-custom-span-evaluation-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-22T18:40:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/langwatch-custom-span-evaluation-evidence/map_to_assay.py \
  examples/langwatch-custom-span-evaluation-evidence/fixtures/failure.langwatch.json \
  --output examples/langwatch-custom-span-evaluation-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-22T18:41:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/langwatch-custom-span-evaluation-evidence/map_to_assay.py \
  examples/langwatch-custom-span-evaluation-evidence/fixtures/malformed.langwatch.json \
  --output /tmp/langwatch-malformed.assay.ndjson \
  --import-time 2026-04-22T18:42:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
carry a wider trace envelope into a single-evaluation v1 lane.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat LangWatch evaluation, trace, or workflow semantics as Assay truth
- partially import larger trace or evaluation-session wrappers
- import dataset, prompt, or scenario truth
- claim that the reduced artifact is a stable upstream wire-format contract

This sample targets the smallest honest LangWatch custom evaluation seam, not a
LangWatch platform lane.

## Checked-in fixtures

- `fixtures/valid.langwatch.json`: bounded positive custom evaluation artifact
- `fixtures/failure.langwatch.json`: bounded negative custom evaluation artifact
- `fixtures/malformed.langwatch.json`: malformed trace-envelope import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
