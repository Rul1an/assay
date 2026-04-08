# A2A Task Evidence Sample

This example turns a tiny A2A task lifecycle export into bounded, reviewable
external evidence for Assay.

It is intentionally small:

- start with `task.requested` and `task.updated`
- keep route visibility to one bounded optional `route_ref` only when it is
  already naturally present in the chosen artifact
- freeze one success artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep A2A task outcomes, delegation correctness, and trust/accountability
  semantics as observed protocol evidence, not Assay truth

## What is in here

- `map_to_assay.py`: turns exported A2A NDJSON records into Assay-shaped
  placeholder envelopes
- `fixtures/`: one success artifact, one failure artifact, one malformed
  artifact, and mapped sample output

## Why this seam

This sample treats task lifecycle as the current best first seam for an
external evidence consumer. `route_ref` is secondary here and is carried only
as a bounded observed reference when it is already naturally present.

That keeps the sample small and avoids turning the first outward move into a
broader claim about:

- discovery cards
- identity or authorization
- trust/accountability primitives
- route correctness or delegation correctness

## Map the checked-in success artifact

```bash
python3 map_to_assay.py \
  fixtures/valid.a2a.ndjson \
  --output fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T15:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 map_to_assay.py \
  fixtures/failure.a2a.ndjson \
  --output fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T15:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 map_to_assay.py \
  fixtures/malformed.a2a.ndjson \
  --output /tmp/a2a-malformed.assay.ndjson \
  --import-time 2026-04-07T15:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat A2A task outcomes, delegation correctness, or trust/accountability
  semantics as Assay truth
- imply that Assay independently verified the route, the receiving agent, or
  the correctness of the delegation
- claim that this sample already defines the stable external-consumer contract

This is a tiny external-consumer sample, not a proposal to freeze a new A2A
contract or to inherit protocol semantics as Assay truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, LangGraph, and OpenAI Agents
samples, so the placeholder envelopes are honest about deterministic hashing
without pretending to be a full RFC 8785 canonicalizer for arbitrary JSON
input.

## Checked-in fixtures

- `fixtures/valid.a2a.ndjson`: bounded success export
- `fixtures/failure.a2a.ndjson`: bounded failure export
- `fixtures/malformed.a2a.ndjson`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
