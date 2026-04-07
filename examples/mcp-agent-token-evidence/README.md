# MCP-Agent Runtime Accounting Evidence Sample

This example turns one tiny `mcp-agent` token-summary artifact into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start with one exported token summary from the documented token-counter path
- keep the sample to one valid artifact, one failure artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep token counts, model breakdowns, and cost estimates as observed runtime accounting only
- keep MCP protocol semantics, workflow correctness, and billing truth out of Assay truth

## What is in here

- `map_to_assay.py`: turns one `mcp-agent` token-summary artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.mcp-agent.json`: one completed runtime-accounting artifact
- `fixtures/failure.mcp-agent.json`: one failed runtime-accounting artifact
- `fixtures/malformed.mcp-agent.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed import time

## Why this seam

This sample treats token summary exported from the documented token-counter path
as the current best first seam hypothesis for `mcp-agent`.

That keeps the first slice on bounded runtime accounting only. It does not turn
the sample into:

- MCP packet capture
- a trace export lane
- a watcher event lane
- a token-tree semantics lane
- a billing or settlement lane

The checked-in fixtures are deliberately docs-backed and frozen. The upstream
`mcp-agent` runnable examples for token counting pull in live provider and app
runtime setup, so this sample keeps the evidence seam honest without pretending
that a live local generator is already the smallest stable path.

The checked-in fixtures also omit `tree_ref` entirely on purpose. `tree_ref`
may exist later as an opaque reference, but v1 keeps the sample on aggregate
runtime accounting only.

## Map the checked-in valid artifact

```bash
python3 examples/mcp-agent-token-evidence/map_to_assay.py \
  examples/mcp-agent-token-evidence/fixtures/valid.mcp-agent.json \
  --output examples/mcp-agent-token-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T18:00:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/mcp-agent-token-evidence/map_to_assay.py \
  examples/mcp-agent-token-evidence/fixtures/failure.mcp-agent.json \
  --output examples/mcp-agent-token-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T18:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/mcp-agent-token-evidence/map_to_assay.py \
  examples/mcp-agent-token-evidence/fixtures/malformed.mcp-agent.json \
  --output /tmp/mcp-agent-malformed.assay.ndjson \
  --import-time 2026-04-07T18:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat token accounting, workflow outcomes, or cost estimates as Assay truth
- imply that Assay independently verified upstream billing or workflow correctness
- define an MCP protocol evidence record
- claim that this sample already defines the stable external-consumer contract

This sample targets the smallest honest runtime-accounting surface exposed by
an MCP-native framework, not an MCP protocol record.

We are not asking Assay to inherit `mcp-agent` token accounting, workflow
outcomes, or runtime semantics as truth.

For the checked-in fixture corpus, the mapper preserves deterministic hashing
for these committed sample inputs and outputs, including the numeric forms
present in the fixtures, without pretending to enforce a narrower integer-only
subset or to be a full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.mcp-agent.json`: bounded completed runtime-accounting export
- `fixtures/failure.mcp-agent.json`: bounded failed runtime-accounting export
- `fixtures/malformed.mcp-agent.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
