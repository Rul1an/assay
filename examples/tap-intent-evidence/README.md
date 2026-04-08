# Visa TAP Verification Evidence Sample

This example turns one tiny frozen artifact derived from the documented Visa
Trusted Agent Protocol signature-verification path into bounded, reviewable
external evidence for Assay.

It is intentionally small:

- start with one frozen verification-outcome artifact shape derived from the
  TAP verification ingredients documented in the public repo
- keep the sample to one valid artifact, one invalid artifact, and one
  malformed case
- map the good artifacts into Assay-shaped placeholder envelopes
- keep verification outcome, signature metadata, domain binding, and operation
  type as observed protocol data only
- keep payment, checkout, customer identity, and merchant-decision semantics
  out of Assay truth

## What is in here

- `map_to_assay.py`: turns one tiny TAP verification artifact into an
  Assay-shaped placeholder envelope
- `fixtures/valid.tap.json`: one successful verification-outcome artifact
- `fixtures/invalid.tap.json`: one rejected verification-outcome artifact
- `fixtures/malformed.tap.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import time
- `fixtures/invalid.assay.ndjson`: mapped placeholder output with a fixed import time

## Why this seam

This sample treats a frozen serialized artifact derived from the documented TAP
signature-verification path as the current best first seam hypothesis for Visa
TAP.

That keeps the first slice on verification outcome only. It does not turn the
sample into:

- a payment-settlement lane
- a checkout-success lane
- a customer-identity lane
- a merchant fraud-decision lane
- a full TAP demo recreation

The checked-in fixtures are deliberately docs-backed and frozen. The public TAP
demo is a multi-component stack with frontend, backend, registry, CDN proxy,
and agent pieces, so this sample keeps the evidence seam honest without
pretending that a whole live commerce stack is already the smallest stable path
in this repo.

The checked-in fixtures also omit every optional reference on purpose. Those
fields may exist later in a bounded sample shape, but v1 keeps the seam on
verification outcome, small signature metadata, merchant-domain binding, and
operation type only.

## Map the checked-in valid artifact

```bash
python3 examples/tap-intent-evidence/map_to_assay.py \
  examples/tap-intent-evidence/fixtures/valid.tap.json \
  --output examples/tap-intent-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-08T13:00:00Z \
  --overwrite
```

## Map the checked-in invalid artifact

```bash
python3 examples/tap-intent-evidence/map_to_assay.py \
  examples/tap-intent-evidence/fixtures/invalid.tap.json \
  --output examples/tap-intent-evidence/fixtures/invalid.assay.ndjson \
  --import-time 2026-04-08T13:05:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/tap-intent-evidence/map_to_assay.py \
  examples/tap-intent-evidence/fixtures/malformed.tap.json \
  --output /tmp/tap-malformed.assay.ndjson \
  --import-time 2026-04-08T13:10:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture is missing
required keys.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat TAP payment semantics, merchant decision semantics, customer identity
  semantics, or commerce outcomes as Assay truth
- imply that Assay independently verified consumer authorization correctness,
  payment completion, or merchant acceptance
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest TAP verification-outcome surface, not a
payment settlement record, merchant decision record, or customer-identity
truth surface.

We are not asking Assay to inherit TAP payment semantics, merchant decision
semantics, customer identity semantics, or commerce outcomes as truth.

For the checked-in fixture corpus, the mapper also stays inside the same
deterministic JSON subset used by the other interop samples, so the
placeholder envelopes are honest about deterministic hashing without pretending
to be a full RFC 8785 canonicalizer for arbitrary JSON input.

## Checked-in fixtures

- `fixtures/valid.tap.json`: bounded valid verification artifact
- `fixtures/invalid.tap.json`: bounded rejected verification artifact
- `fixtures/malformed.tap.json`: malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import time
- `fixtures/invalid.assay.ndjson`: mapped placeholder output with fixed import time
