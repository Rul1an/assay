# LiveKit Agents Testing-Result / RunEvent Evidence Sample

This example turns one tiny frozen artifact derived from LiveKit Agents'
documented `voice.testing.RunResult` surface into bounded, reviewable external
evidence for Assay.

It is intentionally small:

- start with one frozen testing-result artifact shape
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the two good artifacts into Assay-shaped placeholder envelopes
- keep `events` as the primary seam
- treat `final_output_ref` as optional bonus context only
- keep telemetry, room metrics, transcripts, and audio payloads out of Assay
  truth

## What is in here

- `map_to_assay.py`: turns one tiny LiveKit testing-result artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.livekit.json`: one completed testing artifact
- `fixtures/failure.livekit.json`: one failed testing artifact
- `fixtures/malformed.livekit.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from
`voice.testing.RunResult.events` as the best first seam hypothesis for LiveKit
Agents.

That keeps the first slice on testing-result artifacts only. It does not turn
the sample into:

- a telemetry export lane
- a session report lane
- a room metrics lane
- a transcript export lane
- a raw audio export lane
- a production runtime correctness lane

The checked-in fixtures are deliberately docs-backed and frozen. LiveKit has a
real realtime runtime, room model, and broader production story, but v1 keeps
the evidence boundary honest without pretending that a live room bootstrap is
already the smallest stable external-consumer path.

The checked-in fixtures also keep `final_output_ref`, `agent_ref`, and
`sdk_version_ref` out of the valid sample on purpose. Those fields may arrive
later in a bounded shape, but v1 keeps the seam centered on one ordered event
list and one small outcome field.

## Map the checked-in valid artifact

```bash
python3 examples/livekit-runresult-evidence/map_to_assay.py \
  examples/livekit-runresult-evidence/fixtures/valid.livekit.json \
  --output examples/livekit-runresult-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-10T11:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/livekit-runresult-evidence/map_to_assay.py \
  examples/livekit-runresult-evidence/fixtures/failure.livekit.json \
  --output examples/livekit-runresult-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-10T11:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/livekit-runresult-evidence/map_to_assay.py \
  examples/livekit-runresult-evidence/fixtures/malformed.livekit.json \
  --output /tmp/livekit-malformed.assay.ndjson \
  --import-time 2026-04-10T11:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture uses a bad
`message.content` shape.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat LiveKit session semantics, room semantics, transcript semantics, or
  runtime correctness semantics as Assay truth
- imply that Assay independently verified a realtime room, speech pipeline, or
  production deployment outcome
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest LiveKit Agents testing-result surface,
not a telemetry, transcript, audio, or session-report surface.

We are not asking Assay to inherit LiveKit session semantics, room
observability semantics, transcript semantics, or runtime correctness semantics
as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.livekit.json`: bounded testing-result artifact with
  `outcome=completed`
- `fixtures/failure.livekit.json`: bounded testing-result artifact with
  `outcome=failed`
- `fixtures/malformed.livekit.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
