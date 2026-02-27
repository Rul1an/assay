# ADR-026 Canonicalization and Hash Boundary (E3A)

## Intent
Freeze a single canonicalization and hashing contract for ADR-026 adapter outputs.

The goal is to make adapter-emitted evidence reproducible across runs and robust
against semantically irrelevant JSON key-order differences, while preserving a
separate raw-payload forensic boundary.

## Scope
In-scope:
- canonical JSON rules for adapter-emitted payload digests
- explicit hash boundary for canonical event payloads
- explicit hash boundary for preserved raw payload bytes
- determinism requirements for key-order independence tests

Out-of-scope:
- runtime implementation of the shared canonicalization utility (E3B)
- workflow changes
- changes to adapter mapping semantics
- changes to raw payload persistence policy frozen in E2A/E2B

## Canonical event payload contract (v1)
Adapter payload digests must be computed over canonical JSON bytes, not over
implementation-defined serializer output.

Canonical JSON rules (v1):
- object keys sorted lexicographically
- nested objects normalized recursively
- arrays preserve semantic order by default
- arrays may be normalized only when the adapter contract defines them as
  order-insensitive sets
- strings and booleans are emitted unchanged
- integers must preserve numeric value without stringifying unless the adapter
  contract already freezes string encoding for that field

## Raw payload hash boundary
`raw_payload_ref.sha256` is computed over the exact raw payload bytes supplied to
the adapter host boundary.

Normative implications:
- semantically equivalent JSON inputs with different key ordering may share the
  same canonical event payload digest
- those same inputs may have different `raw_payload_ref.sha256` values because
  the raw-byte forensic boundary is intentionally stricter

## Determinism requirements
Each adapter conformance suite must include:
- at least one test proving semantically equivalent payloads with different JSON
  key ordering produce the same canonical event payload digest
- at least one test proving exact raw payload bytes remain distinguishable via
  `raw_payload_ref.sha256`
- no duplicate ad hoc digest helpers once the shared canonicalization utility is
  introduced

## Shared utility contract
E3B must introduce a shared canonicalization helper in the adapter layer so ACP
and A2A do not diverge in hashing semantics.

Minimum shared surface:
- `canonical_json_bytes(value) -> Vec<u8>`
- `digest_canonical_json(value) -> sha256 hex`

## Non-goals
- no change to adapter lossiness semantics
- no change to AttachmentWriter media-type or size-cap policy
- no crates.io publication changes
