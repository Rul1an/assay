# x402 Requirement / Verification Evidence Sample

This example turns one tiny frozen artifact derived from x402's current
`PaymentRequired` plus `VerifyResponse` path into bounded, reviewable external
evidence for Assay.

It is intentionally small:

- start with one frozen requirement-and-verification artifact shape
- keep the sample to one valid artifact, one invalid artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep requirement-side amount and asset context as observed upstream data only
- keep settlement, receipts, payer identity, and fulfillment truth out of
  Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny x402 requirement/verification artifact into
  an Assay-shaped placeholder envelope
- `fixtures/valid.x402.json`: one verified artifact
- `fixtures/invalid.x402.json`: one rejected artifact
- `fixtures/malformed.x402.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/invalid.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats a frozen serialized artifact derived from x402's
`PaymentRequired` plus `VerifyResponse` path as the best first seam hypothesis
for x402.

That keeps the first slice on requirement-and-verification artifacts only. It
does not turn the sample into:

- a settlement export lane
- a transaction receipt lane
- a raw payload import lane
- a payer identity lane
- a transport-cross canonicalization lane

The checked-in fixtures are deliberately docs-backed and frozen. x402 has real
HTTP, MCP, and A2A transport surfaces plus facilitator verification and
settlement flows, but v1 keeps the evidence boundary honest without pretending
that a live wallet or facilitator bootstrap is already the smallest stable
path.

The checked-in fixtures also omit `payee_ref`, `facilitator_ref`,
`payment_identifier_ref`, and every other optional top-level reference on
purpose. Those fields may arrive later in a bounded sample shape, but v1 keeps
the seam on one resource reference, one chosen scheme/network pair, one
requirement-side amount/asset pair, and one bounded verification result.

## Map the checked-in valid artifact

```bash
python3 examples/x402-verification-evidence/map_to_assay.py \
  examples/x402-verification-evidence/fixtures/valid.x402.json \
  --output examples/x402-verification-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-09T13:00:00Z \
  --overwrite
```

## Map the checked-in invalid artifact

```bash
python3 examples/x402-verification-evidence/map_to_assay.py \
  examples/x402-verification-evidence/fixtures/invalid.x402.json \
  --output examples/x402-verification-evidence/fixtures/invalid.assay.ndjson \
  --import-time 2026-04-09T13:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/x402-verification-evidence/map_to_assay.py \
  examples/x402-verification-evidence/fixtures/malformed.x402.json \
  --output /tmp/x402-malformed.assay.ndjson \
  --import-time 2026-04-09T13:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat x402 requirement semantics, verification semantics, settlement
  semantics, or transport semantics as Assay truth
- imply that Assay independently verified onchain state or settlement finality
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest x402 requirement-and-verification
surface, not a settlement-response, receipt, payer, or fulfillment truth
surface.

We are not asking Assay to inherit x402 settlement semantics, merchant
fulfillment semantics, payer identity semantics, or broader commerce outcomes
as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the placeholder
envelopes are honest about deterministic hashing without pretending to be a
full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.x402.json`: bounded requirement-and-verification artifact
  with `verification_result=verified`
- `fixtures/invalid.x402.json`: bounded requirement-and-verification artifact
  with `verification_result=rejected`
- `fixtures/malformed.x402.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/invalid.assay.ndjson`: mapped placeholder output with fixed import time
