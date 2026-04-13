# Vercel AI SDK UIMessage Evidence Sample

This example turns one tiny frozen wrapper artifact derived from Vercel AI
SDK's documented `UIMessage` surface into bounded, reviewable external
evidence for Assay.

It is intentionally small:

- start with one frozen wrapper artifact derived from bounded `UIMessage`
  records
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep message parts and bounded tool state as observed upstream data only
- keep traces, telemetry, source/file/data parts, and backend truth out of
  Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Vercel AI SDK `UIMessage`-level artifact
  into an Assay-shaped placeholder envelope
- `fixtures/valid.vercel-ai.json`: one bounded successful conversation sample
- `fixtures/failure.vercel-ai.json`: one bounded message/tool-error sample
- `fixtures/malformed.vercel-ai.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from Vercel AI SDK's
`UIMessage` path as the best first seam hypothesis for the SDK.

That keeps the first slice on message artifacts only. It does not turn the
sample into:

- a telemetry export lane
- a tracing export lane
- a provider payload lane
- a file or source attachment lane
- a backend correctness lane

The checked-in fixtures are deliberately docs-backed and frozen. Vercel AI SDK
has real stream protocols, richer part types, metadata, and provider-adjacent
surfaces, but v1 keeps the evidence boundary honest without pretending that a
live backend or provider round-trip is already the smallest stable path.

The checked-in fixtures also keep the top-level wrapper deliberately small.
Fields like `thread_ref` and the top-level `messages` list belong to the
sample wrapper, not to a claim that Vercel AI SDK exposes one canonical
conversation wrapper contract.

The checked-in fixtures omit every optional top-level field except
`stream_protocol` and small bounded message metadata on purpose. Those fields
may expand later in a bounded sample shape, but v1 keeps the seam on one
optional wrapper reference, one bounded message list, and one small subset of
`text` and `tool-*` parts.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded message
artifact that includes a tool error state, not an infrastructure failure.

## Map the checked-in valid artifact

```bash
python3 examples/vercel-ai-uimessage-evidence/map_to_assay.py \
  examples/vercel-ai-uimessage-evidence/fixtures/valid.vercel-ai.json \
  --output examples/vercel-ai-uimessage-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-12T21:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/vercel-ai-uimessage-evidence/map_to_assay.py \
  examples/vercel-ai-uimessage-evidence/fixtures/failure.vercel-ai.json \
  --output examples/vercel-ai-uimessage-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-12T21:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/vercel-ai-uimessage-evidence/map_to_assay.py \
  examples/vercel-ai-uimessage-evidence/fixtures/malformed.vercel-ai.json \
  --output /tmp/vercel-ai-malformed.assay.ndjson \
  --import-time 2026-04-12T21:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture falls
outside the bounded `UIMessage` subset used by this sample.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Vercel AI SDK message semantics, tool semantics, or streaming
  semantics as Assay truth
- imply that Assay independently verified backend correctness or tool
  correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest `UIMessage`-derived surface, not a
trace export, telemetry export, provider payload capture, or backend truth
surface.

We are not asking Assay to inherit UI semantics, tool semantics, or streaming
semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same small,
deterministic JSON profile used by the other interop samples. It is honest
about deterministic hashing for this sample corpus without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.vercel-ai.json`: bounded wrapper artifact with `UIMessage`
  text and tool output parts
- `fixtures/failure.vercel-ai.json`: bounded wrapper artifact with a tool
  error part
- `fixtures/malformed.vercel-ai.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
