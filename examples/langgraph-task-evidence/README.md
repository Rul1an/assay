# LangGraph Task Evidence Sample

This example turns a tiny LangGraph `stream(..., stream_mode="tasks", version="v2")`
artifact into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start with LangGraph's OSS-native `tasks` stream mode
- treat that surface as the current best first seam hypothesis, subject to maintainer confirmation
- keep the sample to one success artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep LangGraph task outcomes and orchestration behavior as observed runtime evidence, not Assay truth

## What is in here

- `requirements.txt`: pinned LangGraph dependency for the sample generator
- `generate_synthetic_run.py`: emits a tiny exported task artifact from a real LangGraph graph
- `map_to_assay.py`: turns exported records into Assay-shaped placeholder envelopes
- `fixtures/`: one success artifact, one failure artifact, one malformed artifact, and mapped sample output

## Why this seam

This sample treats `tasks` in stream v2 as the current best OSS-native first seam,
subject to maintainer confirmation.

The checkpointer is only an enabling dependency for this sample. It is not the
interop seam we are testing, and this example does not try to turn LangGraph
persistence semantics into Assay semantics.

## Install

```bash
cd examples/langgraph-task-evidence
python3.13 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

## Generate a success artifact

```bash
python generate_synthetic_run.py \
  --scenario success \
  --output fixtures/generated-success.langgraph.ndjson \
  --overwrite
```

## Generate a failure artifact

```bash
python generate_synthetic_run.py \
  --scenario failure \
  --output fixtures/generated-failure.langgraph.ndjson \
  --overwrite
```

The failure path still starts from the real `tasks` stream. If the run aborts on
an exception before LangGraph emits a terminal task-result part, the exporter
adds one small terminal `stream_error` record so the artifact does not silently
drop the broken run. That `stream_error` record is exporter-added after reading
the failed task state from the required checkpointer; it is not itself a native
LangGraph `tasks` stream part.

## Map the export into Assay-shaped placeholder evidence

```bash
python map_to_assay.py \
  fixtures/success.langgraph.ndjson \
  --output fixtures/success.assay.ndjson \
  --import-time 2026-04-07T11:00:00Z \
  --overwrite
```

## Check the malformed case

```bash
python map_to_assay.py \
  fixtures/malformed.langgraph.ndjson \
  --output /tmp/langgraph-malformed.assay.ndjson \
  --import-time 2026-04-07T11:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat LangGraph task outcomes, orchestration behavior, or graph routing as Assay truth
- imply that Assay independently verified graph correctness
- turn checkpoint or persistence semantics into the interop story
- claim that this sample already defines the stable external-consumer contract

This sample does not treat LangGraph task outcomes, orchestration behavior, or
graph routing as Assay truth; it imports only observed runtime evidence from a
bounded streaming surface.

The placeholder event type in `map_to_assay.py` is there so we can test the
handoff honestly without pretending the contract is already frozen.
For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, and CrewAI samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a full
RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/success.langgraph.ndjson`: bounded success export
- `fixtures/failure.langgraph.ndjson`: bounded failure export
- `fixtures/malformed.langgraph.ndjson`: malformed import case
- `fixtures/success.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
