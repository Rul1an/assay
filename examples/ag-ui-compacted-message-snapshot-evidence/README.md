# AG-UI Compacted Message Snapshot Evidence Sample

This example turns one tiny frozen artifact derived from an AG-UI
snapshot-emitting run envelope into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start from one bounded run envelope with one compacted message-history seam
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the two good artifacts into Assay-shaped placeholder envelopes
- keep thread/run anchors, compacted text-bearing messages, and one terminal
  run label at the center
- keep state sync, replay, transport, activity, and broader serialization
  semantics out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny AG-UI compacted-message-history artifact
  into an Assay-shaped placeholder envelope
- `fixtures/valid.ag-ui.json`: one bounded successful run-envelope sample
- `fixtures/failure.ag-ui.json`: one bounded failed run-envelope sample
- `fixtures/malformed.ag-ui.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen artifact derived from one serialized AG-UI run
envelope with one bounded `MESSAGES_SNAPSHOT` seam as the best first lane
hypothesis for AG-UI.

That keeps the first slice on portable compacted message history only. It does
not turn the sample into:

- an AG-UI event-stream import path
- an AG-UI serialization implementation claim
- a replay-completeness lane
- a frontend state synchronization lane
- a protocol-fidelity lane

The checked-in fixtures are deliberately frozen and smaller than the full AG-UI
model. AG-UI messages, lifecycle events, lineage fields, and state management
surfaces are richer than this sample. V1 keeps the evidence boundary honest
without pretending that Assay now models AG-UI serialization as a whole.

## Current upstream seam

This sample models the current snapshot seam as carefully as we can see it
today.

What that means in practice:

- upstream names `MESSAGES_SNAPSHOT` as the portable conversation-history
  event
- upstream serialization docs are broader than this sample and also cover
  lineage, state compaction, input normalization, restore, and time travel
- the current JavaScript `compactEvents` helper reduces verbose text and tool
  call sequences, but it does **not** generate `MESSAGES_SNAPSHOT`

So this is a bounded mapping lane for one compacted message-history artifact,
not a claim that AG-UI already ships one canonical export wrapper for external
evidence consumers.

The first real proof target for this lane is the official ADK middleware path
with `emit_messages_snapshot=True`, because that is the clearest documented
snapshot-emission path upstream currently exposes.

That proof work is still open. The checked-in fixtures are sample artifacts,
not yet captured end-to-end live-proof artifacts.

## Why the message subset is smaller than AG-UI messages

AG-UI messages can carry more than this sample allows.

The full upstream model includes things like:

- multimodal user content
- encrypted reasoning content
- activity messages
- tool-call structure attached to assistant messages

This sample deliberately does **not** import all of that.

V1 keeps each message to the smallest portable text-bearing subset:

- `id`
- `role`
- `content`
- optional `name`

That keeps the lane on compacted portable message history, not on AG-UI's full
conversation-state model.

## Map the checked-in valid artifact

```bash
python3 examples/ag-ui-compacted-message-snapshot-evidence/map_to_assay.py \
  examples/ag-ui-compacted-message-snapshot-evidence/fixtures/valid.ag-ui.json \
  --output examples/ag-ui-compacted-message-snapshot-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-14T20:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/ag-ui-compacted-message-snapshot-evidence/map_to_assay.py \
  examples/ag-ui-compacted-message-snapshot-evidence/fixtures/failure.ag-ui.json \
  --output examples/ag-ui-compacted-message-snapshot-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-14T20:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/ag-ui-compacted-message-snapshot-evidence/map_to_assay.py \
  examples/ag-ui-compacted-message-snapshot-evidence/fixtures/malformed.ag-ui.json \
  --output /tmp/ag-ui-malformed.assay.ndjson \
  --import-time 2026-04-14T20:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture tries to
pull `state` into the top-level artifact. That is an explicit product-boundary
rejection, not just parser hygiene:

- this lane is not the AG-UI state-management lane
- a free top-level `state` bag would silently widen the claim surface
- if we need state later, that should be a different lane or an explicitly
  narrowed future slice

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat AG-UI message semantics, state semantics, or replay semantics as Assay
  truth
- imply that Assay independently verified protocol completeness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest compacted message-history artifact, not
the AG-UI stream, state, replay, or serialization model as a whole.

Additional caps in this sample:

- `thread_id_ref`, `run_id_ref`, and `parent_run_id_ref` must stay opaque ids,
  not URLs
- `messages` must stay text-bearing and bounded
- `terminal_event` stays on `RUN_FINISHED` or `RUN_ERROR`
- `error_message` is only allowed for `RUN_ERROR`

We are not asking Assay to inherit AG-UI stream fidelity, state sync,
branching correctness, or replay completeness as truth.

For the checked-in fixture corpus, the mapper also stays inside the same small,
deterministic JSON profile the other interop samples use. It is honest about
deterministic hashing for this sample corpus without pretending to be a full
RFC 8785 canonicalizer for arbitrary JSON inputs.

## Checked-in fixtures

- `fixtures/valid.ag-ui.json`: bounded successful run envelope with one
  compacted message-history artifact
- `fixtures/failure.ag-ui.json`: bounded failed run envelope with one compacted
  message-history artifact
- `fixtures/malformed.ag-ui.json`: malformed import case that wrongly tries to
  pull `state` into the lane
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
