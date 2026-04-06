# OpenAI Agents Trace Evidence Sample

This example turns one tiny local OpenAI Agents tracing export into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start with the SDK's `TraceProcessor` extension point, not hooks or sessions
- keep the export local by replacing the default trace processors
- keep the sample to one success artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep Agents SDK runtime judgments and trace semantics as observed metadata, not Assay truth

## What is in here

- `requirements.txt`: pinned OpenAI Agents SDK dependency for the sample generator
- `generate_synthetic_run.py`: emits a tiny exported trace artifact through a custom `TraceProcessor`
- `map_to_assay.py`: turns exported records into Assay-shaped placeholder envelopes
- `fixtures/`: one success artifact, one failure artifact, one malformed artifact, and mapped sample output

## Why this seam

This sample treats `TraceProcessor` as the current best first seam hypothesis,
subject to maintainer confirmation.

The sample uses a local custom model so it can exercise the real `Runner.run(...)`
and tool execution path without requiring an API key or sending traces to the
OpenAI backend. Because of that, the sample stays focused on the trace processor
surface itself rather than on hosted model or dashboard behavior.

## Install

```bash
cd examples/openai-agents-trace-evidence
python3.13 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

## Generate a success artifact

```bash
python generate_synthetic_run.py \
  --scenario success \
  --output fixtures/generated-success.openai-agents.ndjson \
  --overwrite
```

## Generate a failure artifact

```bash
python generate_synthetic_run.py \
  --scenario failure \
  --output fixtures/generated-failure.openai-agents.ndjson \
  --overwrite
```

The failure route uses the same tiny local model and tool-call path, but the
tool raises a local error. The SDK still emits a bounded function span with an
error payload, and the trace still ends cleanly. That is enough for this sample
without turning tool failure semantics into a larger policy story.

## Map the export into Assay-shaped placeholder evidence

```bash
python map_to_assay.py \
  fixtures/success.openai-agents.ndjson \
  --output fixtures/success.assay.ndjson \
  --import-time 2026-04-07T13:00:00Z \
  --overwrite
```

## Check the malformed case

```bash
python map_to_assay.py \
  fixtures/malformed.openai-agents.ndjson \
  --output /tmp/openai-agents-malformed.assay.ndjson \
  --import-time 2026-04-07T13:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Agents SDK runtime judgments, handoff choices, or trace semantics as Assay truth
- imply that Assay independently verified agent correctness
- rely on full sensitive span payload capture
- claim that this sample already defines the stable external-consumer contract

This is a tiny external-consumer sample, not a proposal to freeze a new SDK
contract or to inherit Agents SDK runtime semantics as Assay truth.

We are not asking Assay to inherit Agents SDK runtime judgments, handoff
choices, or trace semantics as truth.

The sample sets `trace_include_sensitive_data=False` and exports only a bounded
subset of the observed trace surface. It does not rely on full inputs/outputs
from generation or function spans, even though the tracing system can carry
that data in other configurations.

The placeholder event type in `map_to_assay.py` is there so we can test the
trace seam honestly without pretending the contract is already frozen.
For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, and LangGraph samples, so the
placeholder envelopes are honest about deterministic hashing without pretending
to be a full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/success.openai-agents.ndjson`: bounded success export
- `fixtures/failure.openai-agents.ndjson`: bounded failure export
- `fixtures/malformed.openai-agents.ndjson`: malformed import case
- `fixtures/success.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
