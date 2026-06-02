# MCP Execution Record Fallback Binding Plan

> **Status:** docs-first slice plan. This page scopes the next
> `assay evidence verify-mcp-records` capability after Check B support.
> It is not a shipped CLI contract yet. The slice adds a no-attestation
> request-envelope fallback path for SEP-2828-style execution records,
> while preserving Assay's current boundary as an independent consumer
> verifier. It does not add signature verification, issuer trust,
> policy correctness, runtime side-effect truth, or supersession
> ordering.

## Why This Slice

The current verifier handles the SEP-2787-backed path:

```text
SEP-2787 attestation -> decision record -> optional outcome record
```

That path is useful when a signed request attestation exists, but many
MCP deployments will not have SEP-2787 at all. SEP-2828's fallback path
uses the same execution-record `backLink` fields, but interprets them
as an observed request-envelope binding:

```text
tools/call params plus _meta -> decision record -> optional outcome record
```

In that fallback mode, `backLink.attestationDigest` is a SHA-256 digest
over the JCS-canonical request envelope the server observed, and
`backLink.attestationNonce` is a separate server-chosen per-call nonce.
The nonce is not part of the request-envelope digest.

The capability value is broader than one conformance fixture: Assay can
evaluate execution-record pairing for non-SEP-2787 deployments without
pretending that fallback binding is attestation provenance.

## Slice Scope

Add a second input mode to `assay evidence verify-mcp-records`:

```bash
assay evidence verify-mcp-records \
  --request-envelope tools-call-envelope.json \
  --decision server-decision-record.json \
  --outcome server-outcome-record.json \
  --format json
```

`--attestation` and `--request-envelope` are mutually exclusive. One of
them is required.

In scope:

- compute the fallback binding digest from the JCS-canonical request
  envelope
- compare decision and outcome `backLink.attestationDigest` values
  against that digest in fallback mode
- compare decision and outcome `backLink.attestationNonce` values
  against each other in fallback mode
- keep Check B shared across modes:
  `outcomeDerived.decisionDigest` must match the digest of the full
  signed decision record
- emit a report that makes the binding mode explicit
- document that fallback verifies an observed request-envelope binding,
  not SEP-2787 provenance

Out of scope:

- supersession or multi-decision ordering
- signature verification or issuer-key trust
- policy correctness
- runtime side-effect truth
- payload/result disclosure
- broad MCP request parsing beyond the supplied envelope artifact
- treating request-envelope fallback as equivalent to signed
  SEP-2787 attestation

## Input Shape

The fallback input artifact is the request envelope defined by the
SEP-2828 fallback text: the `tools/call` params plus `_meta`, encoded as
JSON and canonicalized with RFC 8785 JCS before hashing.

The command should not hash a broader JSON-RPC request object unless
the caller deliberately supplied that broader object as the envelope.
Docs should make the expected shape explicit so reviewers know which
bytes are being bound.

The verifier does not need new `backLink` parsing. Both modes continue
to read:

```json
{
  "backLink": {
    "attestationDigest": "sha256:...",
    "attestationNonce": "server-chosen-nonce-001"
  }
}
```

Only the expected source of `attestationDigest` changes:

| Mode | Expected digest source | Expected nonce source |
|---|---|---|
| SEP-2787 attestation | JCS digest of attestation | `attestation.issuerAsserted.nonce` |
| Request-envelope fallback | JCS digest of request envelope | decision/outcome `backLink.attestationNonce` consistency |

## Report Shape Direction

Keep the existing report schema stable enough for current consumers, but
add a binding block that names the mode:

```json
{
  "schema": "assay.mcp.execution-record-pairing.report.v0",
  "binding": {
    "mode": "request_envelope",
    "digest": "sha256:...",
    "digest_source": "request_envelope_jcs",
    "nonce": "server-chosen-nonce-001",
    "nonce_source": "record_backlink_consistency"
  }
}
```

For the SEP-2787 path, `binding.mode` should be
`sep2787_attestation`, with `digest_source` set to
`sep2787_attestation_jcs` and `nonce_source` set to
`issuerAsserted.nonce`. The existing `attestation` report field can stay
for compatibility when an attestation input is supplied, and should be
`null` in request-envelope mode. The `binding` block is additive;
existing v0 consumers that ignore unknown fields should still be able to
parse the report.

Fallback-specific check ids should say what was verified:

| Check id | Meaning |
|---|---|
| `decision_request_envelope_digest_match` | Decision backLink digest matches the request-envelope digest |
| `decision_request_envelope_nonce_present` | Decision backLink carries the server-chosen nonce used for fallback pairing |
| `outcome_request_envelope_digest_match` | Outcome backLink digest matches the request-envelope digest |
| `decision_outcome_backlink_match` | Decision and outcome describe the same call instance through digest + nonce |
| `outcome_decision_digest_match` | Outcome commits to this full signed decision record |

The current Check B helper should be documented in code as digesting the
full signed decision record, matching SEP-2828 Check B.

## Tests

Add targeted CLI tests for the fallback mode:

1. valid request-envelope fallback with decision + outcome exits `0`
2. decision fallback digest substitution exits `2` and reports
   `decision_request_envelope_digest_match`
3. outcome fallback digest substitution exits `2` and reports
   `outcome_request_envelope_digest_match`
4. outcome nonce substitution exits `2` through
   `decision_outcome_backlink_match`
5. existing SEP-2787 attestation tests still pass unchanged

The test fixtures should compute the request-envelope digest with the
same JCS helper used by the verifier. That proves command wiring and
mode selection. SEP-owned external fixtures remain the conformance
anchor once they carry the observed request envelope.

## Vaara Fixture Interop Note

The current Vaara `fallback_envelope_binding` case carries
`decision.json`, `receipt.json`, `receipt_replayed.json`, and
`expected.json`, but it does not yet commit the observed request
envelope. Assay can implement and test the no-attestation input path now,
but it cannot independently evaluate that external fallback digest until
the envelope artifact exists in the fixture suite.

Correct public framing after this slice:

```text
Assay now has the no-attestation request-envelope input path. The
current external fallback fixture still needs the observed request
envelope committed before Assay can independently evaluate the fallback
digest.
```

## Non-Claims

- Fallback binding is not SEP-2787 provenance.
- Matching a request-envelope digest does not prove the server observed
  that envelope honestly.
- Fallback nonce consistency proves the decision and outcome share a
  server-chosen nonce; it does not prove that nonce was unique or fresh
  for the call.
- Matching decision/outcome records does not prove policy correctness.
- Matching an outcome does not prove runtime side effects happened.
- Assay remains an independent consumer verifier; it does not emit
  records, proxy MCP, or establish issuer trust.

## Slice Gate

Review-ready when:

- `verify-mcp-records` accepts exactly one of `--attestation` or
  `--request-envelope`
- both modes produce an explicit `binding.mode`
- SEP-2787-backed behavior is unchanged
- fallback digest and nonce checks are covered by CLI tests
- Check B remains shared across both modes
- docs state the request-envelope shape and the fallback non-claims
- no supersession, signature verification, or issuer-trust behavior is
  introduced in this slice
