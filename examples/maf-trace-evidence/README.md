# Microsoft Agent Framework Trace Evidence Sample

This example turns one tiny exported Microsoft Agent Framework trace artifact
into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with one exported OpenTelemetry-style trace surface
- keep the sample to one valid artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep trace ids, span ids, timing, status, and span attributes as observed metadata
- keep Agent Framework runtime and governance semantics out of Assay truth

## What is in here

- `map_to_assay.py`: turns exported MAF NDJSON records into Assay-shaped
  placeholder envelopes
- `fixtures/`: one valid artifact, one failure artifact, one malformed artifact,
  and mapped sample output

## Why this seam

This sample treats exported OpenTelemetry-style traces as the current best first
seam hypothesis, subject to maintainer confirmation.

It stays deliberately narrow. The sample does not try to turn workflow meaning,
routing meaning, tool policy meaning, or successful spans into Assay truth. It
tests one honest handoff only: can a bounded exported trace surface be imported
as external evidence at all?

The checked-in fixtures also keep `EnableSensitiveData` effectively false. They
export span metadata and bounded attributes only, not raw inputs, outputs, or
message content.

## Map the checked-in valid artifact

```bash
python3 map_to_assay.py \
  fixtures/valid.maf.ndjson \
  --output fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T17:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 map_to_assay.py \
  fixtures/failure.maf.ndjson \
  --output fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T17:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 map_to_assay.py \
  fixtures/malformed.maf.ndjson \
  --output /tmp/maf-malformed.assay.ndjson \
  --import-time 2026-04-07T17:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Agent Framework runtime judgments, orchestration semantics, or policy meaning as Assay truth
- imply that Assay independently verified runtime correctness
- claim that this sample already defines the stable external-consumer contract

This is a tiny external-consumer sample, not a proposal to freeze a new Agent
Framework contract or to inherit runtime semantics as Assay truth.

We are not asking Assay to inherit Agent Framework runtime judgments, policy
meaning, or higher-level orchestration semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, LangGraph, OpenAI Agents,
A2A, and UCP samples, so the placeholder envelopes are honest about
deterministic hashing without pretending to be a full RFC 8785 canonicalizer
for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.maf.ndjson`: bounded valid export
- `fixtures/failure.maf.ndjson`: bounded failure export
- `fixtures/malformed.maf.ndjson`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
