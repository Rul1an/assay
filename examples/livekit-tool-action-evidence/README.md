# LiveKit Agents Tool Action Evidence Sample

This example turns one tiny frozen artifact derived from LiveKit Agents'
`FunctionToolsExecutedEvent` surface into Assay tool-action receipts.

It is intentionally small:

- start with one Assay-side frozen export shape derived from one
  `function_tools_executed` event
- emit one receipt per function call / output pair
- pair calls and outputs by `call_id` when present
- hash raw arguments and outputs instead of copying them into Assay output
- keep transcripts, audio, room state, usage telemetry, and full traces out of
  the evidence boundary

## What is in here

- `map_to_assay.py`: fixture-only sketch retained from P47 planning; the Rust
  CLI importer below is the canonical Stage 1 path
- `fixtures/valid.livekit.json`: one successful function tool execution event
- `fixtures/failure.livekit.json`: one function tool execution event whose
  output reports an error
- `fixtures/malformed.livekit.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: pre-Stage-1 placeholder output with a fixed
  import time
- `fixtures/failure.assay.ndjson`: pre-Stage-1 placeholder output with a fixed
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

## Import the checked-in valid artifact

```bash
assay evidence import livekit-tool-action \
  --input examples/livekit-tool-action-evidence/fixtures/valid.livekit.json \
  --bundle-out /tmp/livekit-tool-action-valid.tar.gz \
  --source-artifact-ref examples/livekit-tool-action-evidence/fixtures/valid.livekit.json \
  --import-time 2026-05-09T10:00:02Z \
  --run-id livekit_tool_action_valid

assay evidence verify /tmp/livekit-tool-action-valid.tar.gz
```

## Import the checked-in failure artifact

```bash
assay evidence import livekit-tool-action \
  --input examples/livekit-tool-action-evidence/fixtures/failure.livekit.json \
  --bundle-out /tmp/livekit-tool-action-failure.tar.gz \
  --source-artifact-ref examples/livekit-tool-action-evidence/fixtures/failure.livekit.json \
  --import-time 2026-05-09T10:01:02Z \
  --run-id livekit_tool_action_failure

assay evidence verify /tmp/livekit-tool-action-failure.tar.gz
```

## Check the malformed case

```bash
assay evidence import livekit-tool-action \
  --input examples/livekit-tool-action-evidence/fixtures/malformed.livekit.json \
  --bundle-out /tmp/livekit-tool-action-malformed.tar.gz \
  --import-time 2026-05-09T10:02:02Z \
  --run-id livekit_tool_action_malformed
```

This third command is expected to fail because Stage 1 treats a missing
function-call output as malformed. LiveKit's Python type allows
`FunctionCallOutput | None`; a future production reducer may model that as
`completed=false`, but this importer keeps the first acted-family slice strict.

## Important boundary

This importer writes importer-only receipts.

It does not:

- add a public receipt-family matrix entry
- claim that LiveKit provides this exact wire contract
- claim that the tool call was correct, intended, allowed, or safe
- import transcripts, audio, room state, participant identity, usage telemetry,
  or full traces
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
