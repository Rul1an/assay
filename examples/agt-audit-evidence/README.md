# AGT Audit Evidence Sample

This example turns a tiny frozen AGT-style audit corpus into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start with raw `mcp-trust-proxy`-style audit decisions
- keep the corpus to one allow, one deny, and one malformed case
- map the good records into Assay-shaped placeholder envelopes
- keep AGT trust semantics as observed metadata, not Assay truth

## What is in here

- `fixtures/decisions.agt.ndjson`: one allow and one deny audit record
- `fixtures/malformed.agt.ndjson`: one malformed import case
- `fixtures/decisions.assay.ndjson`: mapped placeholder output with a fixed import time
- `map_to_assay.py`: turns AGT NDJSON into Assay-shaped placeholder envelopes

## Why this sample exists

This is the next small step after the AGT interop sketch and discussion:

- keep the seam on raw audit decisions, not broad governance semantics
- keep `trust_score` as observed metadata only
- prove the handoff with a frozen corpus before asking AGT for anything broader

The sample is aligned to the current `mcp-trust-proxy` discussion surface, while
leaving Annex IV and `ComplianceReport` as upstream context rather than
something Assay imports as truth.

## Map the frozen corpus

```bash
python3 examples/agt-audit-evidence/map_to_assay.py \
  examples/agt-audit-evidence/fixtures/decisions.agt.ndjson \
  --output examples/agt-audit-evidence/fixtures/decisions.assay.ndjson \
  --import-time 2026-04-06T18:00:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/agt-audit-evidence/map_to_assay.py \
  examples/agt-audit-evidence/fixtures/malformed.agt.ndjson \
  --output /tmp/agt-malformed.assay.ndjson \
  --import-time 2026-04-06T18:05:00Z \
  --overwrite
```

This second command is expected to fail because the malformed fixture is
missing required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- translate AGT `trust_score` values into Assay trust tiers
- imply that Assay independently verified AGT policy adequacy
- imply that AGT `reason` strings are anything more than observed runtime output

The placeholder event type in `map_to_assay.py` is there so we can test the
handoff honestly without pretending the contract is already frozen.

## Checked-in fixtures

- `fixtures/decisions.agt.ndjson`: frozen allow + deny AGT audit corpus
- `fixtures/malformed.agt.ndjson`: malformed import case
- `fixtures/decisions.assay.ndjson`: mapped placeholder output with fixed import time
