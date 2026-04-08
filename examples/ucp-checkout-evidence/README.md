# UCP Checkout Evidence Sample

This example turns a tiny UCP checkout/order lifecycle export into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start with `order.requested` and `checkout.updated`
- keep the seam on observed order and checkout state only
- freeze one valid artifact, one denied-progress artifact, and one malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep UCP commerce, payment, settlement, and merchant semantics as observed
  protocol evidence, not Assay truth

## What is in here

- `map_to_assay.py`: turns exported UCP NDJSON records into Assay-shaped
  placeholder envelopes
- `fixtures/`: one valid artifact, one denied-progress artifact, one malformed
  artifact, and mapped sample output

## Why this seam

This sample treats checkout/order lifecycle as the current best first seam for
an external evidence consumer.

That keeps the sample small and avoids turning the first outward move into a
broader claim about:

- payment authorization truth
- payment settlement truth
- merchant legitimacy
- fulfillment correctness
- refunds, identity, or marketplace trust

In v1, this sample is deliberately about order-state observation, not commerce
truth. A denied or blocked state in the fixture corpus is still only observed
protocol state.

## Map the checked-in valid artifact

```bash
python3 map_to_assay.py \
  fixtures/valid.ucp.ndjson \
  --output fixtures/valid.assay.ndjson \
  --import-time 2026-04-07T16:00:00Z \
  --overwrite
```

## Map the checked-in denied-progress artifact

```bash
python3 map_to_assay.py \
  fixtures/failure.ucp.ndjson \
  --output fixtures/failure.assay.ndjson \
  --import-time 2026-04-07T16:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 map_to_assay.py \
  fixtures/malformed.ucp.ndjson \
  --output /tmp/ucp-malformed.assay.ndjson \
  --import-time 2026-04-07T16:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat order state, checkout state, payment progress, or merchant semantics as
  Assay truth
- imply that Assay independently verified payment authorization, settlement, or
  legitimacy
- claim that this sample already defines the stable external-consumer contract

This is a tiny external-consumer sample, not a proposal to freeze a new UCP
contract or to inherit protocol semantics as Assay truth.

For the checked-in fixture corpus, the mapper also stays inside the same
JCS-safe subset boundary as the ADK, AGT, CrewAI, LangGraph, OpenAI Agents,
and A2A samples, so the placeholder envelopes are honest about deterministic
hashing without pretending to be a full RFC 8785 canonicalizer for arbitrary
JSON input.

## Checked-in fixtures

- `fixtures/valid.ucp.ndjson`: bounded valid export
- `fixtures/failure.ucp.ndjson`: bounded denied-progress export
- `fixtures/malformed.ucp.ndjson`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import time
