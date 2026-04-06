# CrewAI Event Evidence Sample

This example turns a small CrewAI event-listener export into bounded, reviewable
external evidence for Assay.

It is intentionally small:

- start with CrewAI's local event listener seam
- export a tiny NDJSON artifact
- map that artifact into Assay-shaped placeholder envelopes
- keep CrewAI runtime semantics as observed evidence, not Assay truth

## What is in here

- `export_listener.py`: a bounded CrewAI `BaseEventListener` that writes selected events to NDJSON
- `generate_synthetic_run.py`: emits a short synthetic run through CrewAI's real event bus
- `map_to_assay.py`: turns exported records into Assay-shaped placeholder envelopes
- `fixtures/`: one success artifact, one failure artifact, one malformed artifact, and mapped sample output

## Why the dependency is pinned this way

The current public CrewAI repo is ahead of the simple package-index story right now.
This sample is pinned to the upstream commit we verified against:

- `71b4667a0e12de74b320bffaf4d749ba6bda850c`
- authored on `2026-04-06`

That keeps the sample aligned with the event classes currently exported from `crewai.events`.
The synthetic runner also relies on a guarded reset of CrewAI's global event bus,
because this pinned snapshot does not expose a simple public reset hook for a
self-contained example.

## Install

```bash
cd examples/crewai-event-evidence
python3.13 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

If `python3.13` is not available locally, use another Python in CrewAI's supported
range (`>=3.10, <3.14`).

## Generate a success artifact

```bash
python generate_synthetic_run.py \
  --scenario success \
  --output fixtures/generated-success.crewai.ndjson \
  --overwrite
```

## Generate a failure artifact

```bash
python generate_synthetic_run.py \
  --scenario failure \
  --output fixtures/generated-failure.crewai.ndjson \
  --overwrite
```

## Optional MCP bonus path

The sample does not depend on MCP-specific events for v1, but the exporter also
supports them. If you want a tiny MCP-adjacent run:

```bash
python generate_synthetic_run.py \
  --scenario mcp-success \
  --output fixtures/generated-mcp.crewai.ndjson \
  --overwrite
```

## Map the export into Assay-shaped placeholder evidence

```bash
python map_to_assay.py \
  fixtures/success.crewai.ndjson \
  --output fixtures/success.assay.ndjson \
  --import-time 2026-04-06T11:00:00Z \
  --overwrite
```

## Important boundary

This mapper writes **sample-only placeholder envelopes**.

It does **not**:

- register a new Assay Evidence Contract event type
- claim that imported CrewAI scores, evaluations, or runtime judgments are Assay truth
- claim that the output is a productized Assay interop surface

The placeholder event type in `map_to_assay.py` is there so we can test the handoff
shape honestly without pretending the contract is already frozen.
For the checked-in fixture corpus, the mapper also mirrors Assay's content-hash
input shape so the placeholder envelopes stay honest about deterministic hashing.

## Checked-in fixtures

- `fixtures/success.crewai.ndjson`: bounded success export
- `fixtures/failure.crewai.ndjson`: bounded failure export
- `fixtures/mcp-success.crewai.ndjson`: bounded MCP-adjacent success export
- `fixtures/malformed.crewai.ndjson`: malformed line for mapper failure checks
- `fixtures/success.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
