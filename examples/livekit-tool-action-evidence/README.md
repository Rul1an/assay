# LiveKit Agents Tool Action Evidence Sample

This example turns one tiny frozen artifact derived from LiveKit Agents'
`FunctionToolsExecutedEvent` surface into Assay-shaped placeholder envelopes.

It is intentionally small:

- start with one Assay-side frozen export shape derived from one
  `function_tools_executed` event
- emit one placeholder envelope per function call / output pair
- pair calls and outputs by `call_id` when present
- hash raw arguments and outputs instead of copying them into Assay output
- keep transcripts, audio, room state, usage telemetry, and full traces out of
  the evidence boundary

## What is in here

- `map_to_assay.py`: turns one frozen LiveKit function-tool event artifact into
  Assay-shaped placeholder envelopes
- `fixtures/valid.livekit.json`: one successful function tool execution event
- `fixtures/failure.livekit.json`: one function tool execution event whose
  output reports an error
- `fixtures/malformed.livekit.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

The existing LiveKit example in this repo is P16:
`examples/livekit-runresult-evidence/`. That sample is a testing-result lane
over `voice.testing.RunResult.events`.

This sample is P47. It is narrower: it treats LiveKit function-tool execution
as the first acted-family candidate.

The distinction matters:

- P16 asks what appeared in a testing-result event list.
- P47 asks which tool action was actually observed.

That keeps acted-family work out of broader testing-result, transcript,
telemetry, and room-state surfaces.

## Map the checked-in valid artifact

```bash
python3 examples/livekit-tool-action-evidence/map_to_assay.py \
  examples/livekit-tool-action-evidence/fixtures/valid.livekit.json \
  --output examples/livekit-tool-action-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-05-09T10:00:02Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/livekit-tool-action-evidence/map_to_assay.py \
  examples/livekit-tool-action-evidence/fixtures/failure.livekit.json \
  --output examples/livekit-tool-action-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-05-09T10:01:02Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/livekit-tool-action-evidence/map_to_assay.py \
  examples/livekit-tool-action-evidence/fixtures/malformed.livekit.json \
  --output /tmp/livekit-tool-action-malformed.assay.ndjson \
  --import-time 2026-05-09T10:02:02Z \
  --overwrite
```

This third command is expected to fail because the placeholder sample treats a
missing function-call output as malformed. LiveKit's Python type allows
`FunctionCallOutput | None`; a future production reducer may model that as
`completed=false`, but this example keeps the first acted-family fixture strict.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- claim that LiveKit provides this exact wire contract
- claim that the tool call was correct, intended, allowed, or safe
- import transcripts, audio, room state, participant identity, usage telemetry,
  or full traces
- add a public receipt-family matrix entry
- add a Trust Basis claim

The sample is only a fixture-backed sketch of the LiveKit acted-family seam.
The public family matrix should move only after this shape is proven with a
small fixture set and reviewed separately.

Raw arguments and outputs may appear in the local source fixtures only to prove
hashing behavior. They are fixture inputs, not receipt payload fields.

## Checked-in fixtures

- `fixtures/valid.livekit.json`: bounded function-tool execution event with
  `is_error=false`
- `fixtures/failure.livekit.json`: bounded function-tool execution event with
  `is_error=true`
- `fixtures/malformed.livekit.json`: malformed missing-output import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time and deterministic SHA-256 hashes
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time and deterministic SHA-256 hashes
