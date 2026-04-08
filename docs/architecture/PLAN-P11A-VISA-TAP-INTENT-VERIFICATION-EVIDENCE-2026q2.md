# PLAN — P11A Visa TAP Intent Verification Evidence Interop (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the first `P11` commerce / trust-proof lane
  around Visa Trusted Agent Protocol. No sample implementation, no outward
  issue, no contract freeze in this slice.

## 1. Why this plan exists

After the current wave, the next frontier lane should still pass the same
three tests:

1. the upstream protocol already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream commerce or
   identity semantics as truth,
3. the maintainers have at least one natural place for one small sample-backed
   boundary question.

`visa/trusted-agent-protocol` fits that pattern well enough to justify a formal
plan:

- the protocol is explicitly about trust between agents and merchants in
  agentic commerce
- the repo documents a concrete verification story rather than a vague
  observability story
- the public README calls out timestamps, session identifiers, key identifiers,
  algorithm identifiers, domain binding, operation binding, and replay
  prevention
- issues are enabled, even though Discussions are not

This is the strongest current frontier candidate because it opens a new
protocol class without collapsing back into another framework runtime or eval
lane.

This is **not** a payment-truth plan.

This is **not** a checkout-success plan.

This is **not** a user-identity import plan.

This is a plan for a **bounded TAP signature-verification result seam**.

## 2. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest TAP verification-result surface, not
> a payment settlement record, merchant decision record, or customer-identity
> truth surface.

That means:

- TAP is the upstream protocol context, not the truth source for payment or
  identity claims inside Assay
- signature verification is the first seam, not merchant outcome semantics
- Assay stays an external evidence consumer, not a verifier of economic
  correctness, merchant correctness, or consumer authorization correctness
  beyond the observed artifact

## 3. Why verification-first, not payment-first

The TAP README makes it very tempting to talk about payments because it
mentions payment identifiers, streamlined checkout, and reduced fraud.

That would be the wrong first wedge.

Why:

- it would drag the lane immediately into payment and merchant semantics
- it would make overclaiming much easier
- it would blur the difference between observed verification data and business
  outcome truth
- it would skip the smaller surface already documented in TAP itself: a
  cryptographic verification path bound to time, session, key, domain, and
  operation

The cleaner first wedge is:

- one artifact derived from the documented TAP verification path
- bounded verification result
- bounded signature metadata
- bounded domain and operation binding
- no user identifiers
- no payment identifiers
- no merchant outcome semantics

This keeps `P11A` aligned with Assay’s trust-compiler direction without turning
the sample into a commerce demo.

## 4. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented TAP
signature-verification surface** as the first external-consumer seam.

The publicly documented ingredients of that seam include:

- timestamp
- unique session identifier
- key identifier
- algorithm identifier
- merchant domain binding
- operation binding
- verification result

This is intentionally not:

- checkout result export
- settlement export
- consumer identity export
- PAR or loyalty identifier import
- merchant fraud decision export

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented TAP
> signature-verification surface, not a claim that TAP already guarantees one
> fixed wire-export contract for external evidence consumers.

## 5. v1 artifact contract

### 5.1 Required fields

The first sample should require:

- `schema`
- `protocol`
- `surface`
- `timestamp`
- `session_id`
- `key_id`
- `algorithm`
- `merchant_domain_ref`
- `operation_type`
- `verification_result`

### 5.2 Optional fields

The first sample may include:

- `agent_id_ref`
- `registry_ref`
- `verification_reason`
- `request_ref`

### 5.3 Important field boundaries

#### `verification_result`

This field is required in the frozen sample shape.

It should stay small and bounded:

- `verified`
- `rejected`

If the chosen sample shape needs a little more texture, use a short
`verification_reason`, not a large verifier transcript.

This requirement belongs to the sample shape, not to an upstream claim that
TAP guarantees one universal serialized verification contract.

#### Signature metadata

`timestamp`, `session_id`, `key_id`, and `algorithm` are required because they
are exactly what make the TAP verification story bounded and reviewable.

In v1, these fields must remain:

- observed metadata
- replay-defense context
- verification context

They must not become:

- customer identity truth
- merchant authorization truth
- payment truth

#### `merchant_domain_ref`

This field is required because domain binding is part of the documented TAP
story.

It should stay a bounded reference only:

- hostname
- short domain label

Not allowed in v1:

- full merchant session payload
- full request headers
- checkout body payloads

#### `operation_type`

This field is required because TAP explicitly distinguishes the specific action
being authorized, including browsing or payment.

In v1, keep it small:

- `browsing`
- `payment`

It must remain an observed upstream operation label, not a claim that Assay
independently validated the action.

#### Optional references

The optional reference fields must stay bounded:

- small label
- opaque id
- short reference string

Not allowed in v1:

- verifiable consumer identifiers
- payment account references
- loyalty numbers
- email addresses
- phone numbers
- raw signed request payloads

## 6. Assay-side meaning

The sample may only claim bounded verification observation.

Assay must not treat as truth:

- payment completion
- settlement success
- merchant acceptance
- user identity correctness
- consumer consent correctness beyond the observed verification artifact
- fraud outcome semantics

Common anti-overclaim sentence:

> We are not asking Assay to inherit TAP payment semantics, merchant decision
> semantics, customer identity semantics, or commerce outcomes as truth.

## 7. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/tap-intent-evidence/README.md`
- `examples/tap-intent-evidence/requirements.txt` only if a tiny local
  verifier truly needs it
- `examples/tap-intent-evidence/generate_synthetic_result.py` only if a clean
  local verification generator is viable
- `examples/tap-intent-evidence/map_to_assay.py`
- `examples/tap-intent-evidence/fixtures/valid.tap.json`
- `examples/tap-intent-evidence/fixtures/invalid.tap.json`
- `examples/tap-intent-evidence/fixtures/malformed.tap.json`
- `examples/tap-intent-evidence/fixtures/valid.assay.ndjson`
- `examples/tap-intent-evidence/fixtures/invalid.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional reference field
- v1 fixtures must not include user or payment identifiers
- v1 fixtures must not include raw signed payload bodies
- v1 fixtures should keep the export shape obviously verification-first rather
  than checkout-first

## 8. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 8.1 Preferred path

Preferred:

- a local verifier path that exercises the signature-verification seam only
- no merchant frontend dependency
- no full checkout dependency
- no registry or CDN stack heavy enough to overshadow the sample

### 8.2 Hard fallback rule

If a real local generator would require:

- multiple long-running services
- merchant frontend + backend + proxy + registry orchestration
- cloud dependencies
- consumer or payment test identifiers
- a demo setup heavy enough to dominate the evidence seam

then the sample should fall back to a **docs-backed frozen verification
artifact shape**.

That fallback is especially appropriate here because the public TAP sample is a
multi-component demo. The goal of this lane is to isolate the smallest honest
verification seam, not to recreate the whole commerce stack in this repo.

## 9. Valid, invalid, malformed corpus

The first sample should follow the established corpus pattern.

### 9.1 Valid

One successful verification artifact with:

- bounded signature metadata
- bounded domain and operation binding
- `verification_result=verified`

### 9.2 Invalid

One rejected verification artifact with:

- bounded signature metadata
- bounded domain and operation binding
- `verification_result=rejected`
- one short verification reason if needed

This is not a merchant business denial record. It is only a verification
failure artifact.

### 9.3 Malformed

One malformed artifact that fails fast, for example:

- missing `session_id`
- missing `verification_result`
- unsupported `operation_type`

## 10. Outward strategy

Do not open an outward TAP issue until the sample is on `main`.

After that:

- one small GitHub issue
- one link
- one boundary question
- no broad commerce pitch
- no settlement or identity pitch

Suggested outward question:

> If an external evidence consumer wants the smallest honest TAP surface, is a
> verification-result artifact derived from the signature-verification path
> roughly the right place to start, or is there a thinner verification surface
> you would rather point them at?

## 11. Sequencing rule

`P11A` is strategically above Browser Use in the broader queue, but Browser Use
is still the active adjacent lane already in flight.

Meaning:

1. formalize `P11A` now
2. keep Browser Use on its current finish-and-breathe path
3. decide deliberately whether the next new implementation lane is `P11A` or a
   later adjacent lane after Browser Use settles

## 12. Non-goals

This plan does not:

- define a payment-settlement evidence contract
- define a customer-identity evidence contract
- define a merchant fraud-decision contract
- define a checkout-success contract
- define TAP as Assay truth for commerce outcomes

## References

- [TODO — Next Upstream Interop Lanes (2026 Q2)](./TODO-NEXT-UPSTREAM-INTEROP-LANES-2026q2.md)
- [PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
- [visa/trusted-agent-protocol](https://github.com/visa/trusted-agent-protocol)
