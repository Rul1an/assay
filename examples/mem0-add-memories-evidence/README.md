# Mem0 Add Memories Evidence Sample

This example turns one tiny frozen artifact derived from Mem0's documented
`Add Memories` result path into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start with one frozen mutation-result artifact shape
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep event labels and bounded memory text as observed upstream data only
- keep search, retrieval, graph, and profile truth out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny Mem0 `Add Memories` result artifact into
  an Assay-shaped placeholder envelope
- `fixtures/valid.mem0.json`: one bounded `ADD` result artifact
- `fixtures/failure.mem0.json`: one bounded non-`ADD` result artifact
- `fixtures/malformed.mem0.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from Mem0's current
`Add Memories` result path as the best first seam hypothesis for Mem0.

That keeps the first slice on mutation-result artifacts only. It does not turn
the sample into:

- a search export lane
- a retrieval-truth lane
- a graph export lane
- a profile truth lane
- a prompt or transcript capture lane

The checked-in fixtures are deliberately docs-backed and frozen. Mem0 has
richer memory, search, and graph surfaces, but v1 keeps the evidence boundary
honest without pretending that a live store-backed memory harness is already
the smallest stable path.

The checked-in fixtures do not rely on optional top-level references to define
the seam. Fields such as `user_ref` and `agent_ref` are omitted in this sample
corpus, and `run_ref` may appear in a bounded artifact without changing the
import shape. V1 still keeps the seam on one operation label, one bounded
`results` list, one event label per result, and one short memory string.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid bounded
mutation-result artifact, not an infrastructure failure.

## Map the checked-in valid artifact

```bash
python3 examples/mem0-add-memories-evidence/map_to_assay.py \
  examples/mem0-add-memories-evidence/fixtures/valid.mem0.json \
  --output examples/mem0-add-memories-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-13T12:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/mem0-add-memories-evidence/map_to_assay.py \
  examples/mem0-add-memories-evidence/fixtures/failure.mem0.json \
  --output examples/mem0-add-memories-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-13T12:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/mem0-add-memories-evidence/map_to_assay.py \
  examples/mem0-add-memories-evidence/fixtures/malformed.mem0.json \
  --output /tmp/mem0-malformed.assay.ndjson \
  --import-time 2026-04-13T12:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture violates
the bounded mutation-result shape.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Mem0 memory semantics or mutation semantics as Assay truth
- imply that Assay independently verified memory correctness or profile truth
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest Mem0 mutation-result surface, not a
search export, retrieval truth, graph export, or profile truth surface.

We are not asking Assay to inherit Mem0 memory semantics, retrieval semantics,
or user-profile semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
small, deterministic JSON profile used by the other interop samples. It is
honest about deterministic hashing for this sample corpus without pretending to
be a full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.mem0.json`: bounded mutation-result artifact with one `ADD`
  event
- `fixtures/failure.mem0.json`: bounded mutation-result artifact with one
  `UPDATE` event
- `fixtures/malformed.mem0.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
