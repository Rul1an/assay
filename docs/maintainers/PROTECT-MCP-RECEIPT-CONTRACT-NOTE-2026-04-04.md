# Protect MCP Receipt Contract Note

Date: 2026-04-04

## Purpose

This note records the smallest contract we would need to freeze if the current signed-receipt probe ever graduates beyond probe status.

It is not a product commitment.

## Candidate contract shape

Current working probe shape:

- Passport envelope
  - `payload`
  - `signature`
- verifier boundary
  - `npx @veritasacta/verify@0.2.4`
  - explicit `--key <public_key_hex>` for signed sample receipts

The current bounded receipt fields of interest are:

- `payload.type`
- `payload.tool_name`
- `payload.tool_input_hash`
- `payload.decision`
- `payload.policy_digest`
- `payload.issued_at`
- `payload.issuer_id`
- `payload.spec`
- `payload.claimed_issuer_tier`
- `payload.session_id`
- `payload.sequence`
- `signature.alg`
- `signature.kid`

## Assay-side interpretation

If this ever moves beyond probe status, the current bounded interpretation should stay:

- `receipt_present`
  - derived from envelope presence
- `verification_result`
  - derived by Assay from the pinned verifier boundary
- `issuer_id`
  - observed metadata only
- `claimed_issuer_tier`
  - observed metadata only
- `policy_digest`
  - observed metadata only
- `spec`
  - observed metadata only
- `tool_name`
  - observed metadata only
- `decision`
  - observed metadata only

No trust tier, certification, adequacy, or compliance semantics should be inferred from these fields.

## Binding note

The minimum identity shape we should reason about is:

- `issuer_id + session_id + sequence`

`tool_input_hash` is useful as an additional binding token, but should still be treated cautiously until the interoperability contract is sharper.

For now, the safest reading is:

- `issuer_id + session_id + sequence` identifies the claimed decision slot
- `tool_input_hash` is an opaque adjunct claim about the input bound to that slot

## Open contract gaps

These still need to be tightened before any real seam would be justified:

- `issued_at` remains a signed claim, not a trusted fact
- `tool_input_hash` semantics remain too loose for hard Assay-side reasoning beyond opaque binding
- malformed behavior is still not rich or separately classified by the public verifier CLI
- key handling is still explicit and manual in the working probe path
- the verifier boundary only becomes meaningful if Assay pins version and invocation shape deliberately
