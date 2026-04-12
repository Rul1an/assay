# LlamaIndex EvaluationResult Evidence Sample

This example turns one tiny frozen artifact derived from LlamaIndex's current
`EvaluationResult` surface into bounded, reviewable external evidence for
Assay.

It is intentionally small:

- start with one frozen evaluation-result artifact shape
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep pass/fail, score, and short feedback as observed upstream data only
- keep prompts, completions, traces, callback payloads, and runtime truth out
  of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny LlamaIndex evaluation-result artifact into
  an Assay-shaped placeholder envelope
- `fixtures/valid.llamaindex.json`: one passing evaluation artifact
- `fixtures/failure.llamaindex.json`: one failing evaluation artifact
- `fixtures/malformed.llamaindex.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from LlamaIndex's
`EvaluationResult` path as the best first seam hypothesis for LlamaIndex.

That keeps the first slice on evaluation-result artifacts only. It does not
turn the sample into:

- a tracing export lane
- a callback payload lane
- a prompt or completion capture lane
- a runtime correctness lane
- a benchmark-suite bundle lane

The checked-in fixtures are deliberately docs-backed and frozen. LlamaIndex
has richer evaluator, callback, and workflow surfaces, but v1 keeps the
evidence boundary honest without pretending that a live provider-backed
evaluation harness is already the smallest stable path.

The checked-in fixtures also omit `target_ref`, `invalid_reason`, and every
other optional top-level reference on purpose. Those fields may arrive later in
a bounded sample shape, but v1 keeps the seam on one evaluator label, one
bounded pass/fail outcome, one optional scalar score, and one short feedback
string.

The repo corpus uses `failure` naming to match the established examples
convention. In this lane, that file still represents a valid evaluation
artifact with `passing=false`, not an infrastructure failure.

## Map the checked-in valid artifact

```bash
python3 examples/llamaindex-evalresult-evidence/map_to_assay.py \
  examples/llamaindex-evalresult-evidence/fixtures/valid.llamaindex.json \
  --output examples/llamaindex-evalresult-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-12T20:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/llamaindex-evalresult-evidence/map_to_assay.py \
  examples/llamaindex-evalresult-evidence/fixtures/failure.llamaindex.json \
  --output examples/llamaindex-evalresult-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-12T20:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/llamaindex-evalresult-evidence/map_to_assay.py \
  examples/llamaindex-evalresult-evidence/fixtures/malformed.llamaindex.json \
  --output /tmp/llamaindex-malformed.assay.ndjson \
  --import-time 2026-04-12T20:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture violates
the bounded evaluation-result shape.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat LlamaIndex evaluation semantics or evaluator judgments as Assay truth
- imply that Assay independently verified model quality or task correctness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest LlamaIndex evaluation-result surface,
not a tracing export, callback export, prompt capture, or runtime truth
surface.

We are not asking Assay to inherit evaluator judgments or LlamaIndex
evaluation semantics as truth.

For the checked-in fixture corpus, the mapper also stays inside the same small,
deterministic JSON profile used by the other interop samples. It is honest
about deterministic hashing for this sample corpus without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.llamaindex.json`: bounded evaluation-result artifact with
  `passing=true`
- `fixtures/failure.llamaindex.json`: bounded evaluation-result artifact with
  `passing=false`
- `fixtures/malformed.llamaindex.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
