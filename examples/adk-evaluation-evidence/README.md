# Google ADK Evaluation Evidence Sample

This example turns one tiny Google ADK-style evaluation artifact into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start with one exported evaluation artifact, not runtime traces
- keep the sample to one pass, one fail, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep ADK evaluator semantics as observed metadata, not Assay truth

## What is in here

- `fixtures/valid.adk-eval.json`: one passing evaluation artifact
- `fixtures/failure.adk-eval.json`: one failing evaluation artifact
- `fixtures/malformed.adk-eval.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed import time
- `map_to_assay.py`: turns one ADK evaluation artifact into an Assay-shaped placeholder envelope

## Why this sample exists

This is the next small step after the ADK interop sketch and discussion:

- keep the seam on one exported evaluation artifact
- keep trajectory information at most as an observed artifact reference
- keep evaluator scores, labels, and judgments in the observed bucket
- prove the handoff with a frozen corpus before asking ADK for anything broader

## Map the passing artifact

```bash
python3 examples/adk-evaluation-evidence/map_to_assay.py \
  examples/adk-evaluation-evidence/fixtures/valid.adk-eval.json \
  --output examples/adk-evaluation-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T09:00:00Z \
  --overwrite
```

## Map the failing artifact

```bash
python3 examples/adk-evaluation-evidence/map_to_assay.py \
  examples/adk-evaluation-evidence/fixtures/failure.adk-eval.json \
  --output examples/adk-evaluation-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T09:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/adk-evaluation-evidence/map_to_assay.py \
  examples/adk-evaluation-evidence/fixtures/malformed.adk-eval.json \
  --output /tmp/adk-malformed.assay.ndjson \
  --import-time 2026-04-07T09:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- translate ADK evaluator scores or labels into Assay trust language
- treat trajectory references as a second primary seam
- imply that Assay independently verified evaluator correctness

We are not asking Assay to inherit ADK evaluator scores, runtime judgments, or
trust semantics as truth.

The placeholder event type in `map_to_assay.py` is there so we can test the
handoff honestly without pretending the contract is already frozen.
For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the AGT and CrewAI samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.adk-eval.json`: frozen passing evaluation artifact
- `fixtures/failure.adk-eval.json`: frozen failing evaluation artifact
- `fixtures/malformed.adk-eval.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
