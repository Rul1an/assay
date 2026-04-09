# PLAN — P15 x402 Requirement / Verification Evidence Interop (2026 Q2)

- **Date:** 2026-04-09
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next x402 interop lane after the current
  Browser Use, Visa TAP, Langfuse, and Mastra planning wave. Include a small
  sample implementation, with no outward issue and no contract freeze in this
  slice.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has at least one natural maintainer channel for one small
   sample-backed boundary question.

`x402-foundation/x402` now fits that pattern well enough to justify a formal
plan:

- the canonical upstream repo is current and actively changing
- the core v2 specification exposes a small, concrete requirement and
  verification surface via `PaymentRequired`, `VerifyResponse`, and the
  facilitator interface
- the transport story is already broader than plain HTTP, with current `HTTP`,
  `MCP`, and `A2A` transport specifications
- the protocol now includes adjacent operational extensions such as
  `payment-identifier`, which confirms the ecosystem is moving from demo-only
  flows toward more agent-ready behavior
- the repo clearly supports issues, and the public docs still mention GitHub
  Discussion or Issue as support routes, even though issue-first appears to be
  the cleaner practical path right now

This makes x402 the strongest next new protocol-class candidate after the
recent eval/result-heavy run of Agno, Browser Use, Langfuse, and Mastra.

This is **not** a settlement-truth plan.

This is **not** a merchant fulfillment plan.

This is **not** a payer-identity plan.

This is **not** a transaction-receipt plan.

This is **not** a marketplace metadata plan.

This is a plan for a **bounded x402 requirement-and-verification seam derived
from the documented `PaymentRequired` plus `VerifyResponse` path**.

## 2. Why x402 is a good `P15` candidate

x402 is strategically different from the current adjacent-space candidates.

Why it matters:

- it opens a new protocol family instead of another framework or observability
  lane
- it is explicitly agent-relevant, because the current spec already includes
  `HTTP`, `MCP`, and `A2A` transports
- it has a very small first honest seam available today: payment requirement
  declaration plus verification result
- it fits Assay's trust-compiler direction well, because the first useful
  consumer seam is not "did commerce happen?" but "what bounded requirement and
  verification observation was seen?"

At the same time, the channel shape is weaker than Agno or Browser Use:

- public docs mention GitHub Discussion or Issue as support routes
- issue-first still looks like the cleaner practical route for a small
  sample-backed technical question

That means `P15` is a strong **next planned protocol lane**, but it should
start with a narrow, sample-backed technical issue rather than a broad commerce
pitch.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest x402 requirement-and-verification
> surface, not payment settlement truth, merchant fulfillment truth, payer
> identity truth, or business outcome truth.

That means:

- x402 is the upstream protocol context, not the truth source
- `PaymentRequired` is a stated payment requirement surface, not proof of a
  valid debt or a correct price
- `VerifyResponse` is a bounded verification artifact, not proof that payment
  was settled or that the protected operation should have been fulfilled
- Assay stays an external evidence consumer, not a settlement verifier,
  facilitator authority, or commerce outcome authority

## 4. Why verification-first, not settlement-first

x402 makes it tempting to start with settlement because the protocol also
documents:

- `PAYMENT-RESPONSE`
- `SettlementResponse`
- transaction hashes
- payer fields
- settlement success / failure outcomes

That would be the wrong first wedge.

Why:

- it immediately drags the lane into stronger economic and outcome semantics
- it invites overclaiming around "payment succeeded" when the actual question
  may still be merchant fulfillment, duplicate handling, or route safety
- it increases privacy and identity pressure by pulling payer-facing fields
  earlier than necessary
- it skips the smaller seam that x402 already exposes cleanly: the transition
  from declared requirement to bounded verification result

The cleaner first wedge is:

- one artifact derived from `PaymentRequired`
- one bounded verification result derived from `VerifyResponse`
- one optional short invalid reason when verification fails
- no settlement result
- no transaction receipt
- no payer import

This keeps `P15` aligned with Assay's "observe the bounded proof surface, do
not inherit broader business semantics as truth" rule.

## 5. Why not payload-first

The raw `PaymentPayload` contains much more than Assay needs for a first seam:

- signature material
- authorization parameters
- payer addresses
- validity windows
- nonce / replay-defense data

That is too semantically heavy for v1.

Why:

- it exposes raw signed material that an external evidence consumer does not
  need in order to observe a bounded verification result
- it raises avoidable privacy and operational concerns
- it makes the sample harder to keep deterministic and docs-backed
- it invites downstream consumers to treat raw payment authorizations as a
  portable truth surface rather than as upstream protocol material

The first seam should therefore consume a **reduced requirement-and-verification
artifact derived from** payload plus requirements, not the raw
`PAYMENT-SIGNATURE` header or full payment authorization object itself.

## 6. Why not transport-first

x402 is now broader than plain HTTP:

- `HTTP` transport is documented
- `MCP` transport is documented
- `A2A` transport is documented

That breadth is strategically important, but it is still the wrong place to
start the lane.

Why:

- transport-specific shape differences would make the first sample look wider
  than it needs to be
- the canonical smallest representation is still the `HTTP` flow:
  `PAYMENT-REQUIRED` -> `PAYMENT-SIGNATURE` -> `/verify`
- starting from `MCP` or `A2A` would turn the first wedge into a protocol-cross
  mapping exercise instead of a verification seam
- it is better to prove that the **core x402 verification surface** is honest
  and useful before multiplying it across transports

So for v1:

- the sample should be **HTTP-derived**
- the plan should explicitly acknowledge `MCP` and `A2A` as later follow-up
  expansion paths
- the sample should not claim that the v1 frozen artifact is already the
  canonical cross-transport x402 export shape

## 7. Why not extension-first

x402 already has useful adjacent extensions and proposals:

- `payment-identifier` for idempotency and deduplication
- an open diagnostic extension proposal for richer repeated-failure signaling

Those are real and important, especially for agents.

They are still the wrong first wedge.

Why:

- they are secondary operational layers on top of the core requirement and
  verification path
- they are more likely to evolve than the smaller core verification result
  seam
- they would make the first sample feel ops-heavy instead of protocol-core
- the open diagnostic work is a good sign of active evolution, which is
  exactly why Assay should avoid freezing too much diagnostic meaning in v1

That means:

- `payment_identifier_ref` may be optional in v1 if a frozen artifact naturally
  carries it
- diagnostic and escalation semantics should stay out of v1 unless upstream
  stabilizes them more explicitly

## 8. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented x402
`PaymentRequired` plus `VerifyResponse` path** as the first external-consumer
requirement-and-verification seam.

The intended upstream anchors are:

- `PaymentRequired`
- one chosen `accepts` entry
- facilitator-side `VerifyResponse`
- current `HTTP` transport headers as the narrowest stable representation

The first artifact should stay at the **single verification event** level, not
the full payment lifecycle and not the full commerce workflow.

The first artifact should therefore center on:

- one protocol version
- one transport label
- one bounded resource reference
- one chosen scheme / network pair
- one bounded amount / asset pair
- one bounded requirement context derived from `PaymentRequired`
- one verification result
- one optional invalid reason
- one optional facilitator reference

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented
> `PaymentRequired` plus `VerifyResponse` path, not a claim that x402 already
> guarantees one fixed wire-export contract for external evidence consumers.

## 9. v1 artifact contract

### 9.1 Required fields

The first sample should require:

- `schema`
- `protocol`
- `surface`
- `transport`
- `x402_version`
- `resource_ref`
- `scheme`
- `network`
- `amount`
- `asset_ref`
- `timestamp`
- `verification_result`

These required fields belong to the frozen sample artifact shape.

They must be described as:

- sample-level reductions derived from documented x402 requirement and
  verification surfaces
- not an upstream guarantee that x402 already ships one canonical serialized
  export object with these exact field names

### 9.2 Optional fields

The first sample may include:

- `invalid_reason`
- `invalid_message_ref`
- `facilitator_ref`
- `payee_ref`
- `timeout_seconds`
- `payment_identifier_ref`
- `transport_ref`

### 9.3 Important field boundaries

#### `transport`

This field is required because x402 now spans multiple transports.

In v1, it should stay small:

- `http`

Not allowed in v1:

- multiplexing multiple transport artifacts into one record
- implied claims that `http`, `mcp`, and `a2a` are already one canonical export
  surface for Assay

The field exists to keep later expansion honest, not to widen the first sample.

#### `resource_ref`

This field is required because `PaymentRequired` explicitly binds payment to a
resource.

It must stay a bounded reference only:

- full URL if already public and stable
- short route label
- stable hash or normalized resource reference

Not allowed in v1:

- full request body
- full operation arguments
- server-side business payloads

#### `amount`

This field is required because the payment requirement is not meaningful
without the required amount.

In v1, it must remain:

- an observed required amount in atomic units
- a requirement-side value, not a claim that settlement actually completed for
  that amount
- not a claim about what was ultimately paid
- not a claim about what was economically owed in a broader business sense

It must not become:

- revenue truth
- invoice truth
- fulfillment truth

#### `asset_ref`

This field is required because the requirement is bound to a specific asset.

It should stay bounded:

- contract address
- token symbol if documented in the requirement's `extra`
- ISO code if a fiat form appears in future upstream cases

In v1, it is still requirement-side context only, not a claim about the asset
that was definitively transferred onchain.

Not allowed in v1:

- wallet balances
- account state snapshots
- chain state proofs

#### `verification_result`

This field is required in the frozen sample shape.

It should stay small and bounded:

- `verified`
- `rejected`

If the artifact needs more texture, use `invalid_reason`, not a large
verification transcript.

This requirement belongs to the sample shape, not to a claim that x402
guarantees a universal serialized verification contract with these exact
values.

#### `invalid_reason`

This field is optional in v1.

If present, it must stay a short bounded reason derived from `invalidReason`
or an equivalent upstream verification failure label.

Preferred shape:

- enum or short classifier
- stable failure label where possible

It must not become:

- long free text
- facilitator debug transcript
- a long operator-facing support essay
- an autonomous escalation policy
- a full diagnostics extension contract

That is especially important because richer failure signaling is still being
actively discussed upstream.

#### `payee_ref`

This field is optional in v1.

The sample should prefer omitting it unless the chosen frozen artifact cannot
stay coherent without one bounded payee selector.

If included, it must stay a bounded payee reference only:

- payee address
- short role label if upstream uses one

It must not be promoted into:

- merchant identity truth
- legal entity truth
- fulfillment authority truth

#### `payment_identifier_ref`

This field is optional in v1.

If present, it must stay a bounded idempotency reference only.

It must not become:

- a cross-system correlation truth claim
- a durable customer identifier
- a replay-proof guarantee on its own

## 10. Assay-side meaning

The sample may only claim bounded verification observation.

Assay must not treat as truth:

- settlement completion
- transaction finality
- merchant fulfillment
- payer identity correctness
- operator correctness
- server-side business outcome correctness

Common anti-overclaim sentence:

> We are not asking Assay to inherit x402 settlement semantics, merchant
> fulfillment semantics, payer identity semantics, or broader commerce
> outcomes as truth.

## 11. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/x402-verification-evidence/README.md`
- `examples/x402-verification-evidence/requirements.txt` only if a tiny local
  generator truly needs it
- `examples/x402-verification-evidence/generate_synthetic_result.py` only if a
  clean local generator is viable
- `examples/x402-verification-evidence/map_to_assay.py`
- `examples/x402-verification-evidence/fixtures/valid.x402.json`
- `examples/x402-verification-evidence/fixtures/invalid.x402.json`
- `examples/x402-verification-evidence/fixtures/malformed.x402.json`
- `examples/x402-verification-evidence/fixtures/valid.assay.ndjson`
- `examples/x402-verification-evidence/fixtures/invalid.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional field
- v1 fixtures must not include raw `PAYMENT-SIGNATURE` header contents
- v1 fixtures must not include raw authorization payloads, nonces, or payer
  addresses unless a later review explicitly decides a bounded payer reference
  is necessary
- v1 fixtures must keep the export shape obviously verification-first rather
  than settlement-first

## 12. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

### 12.1 Preferred path

Preferred:

- a local generator that derives one bounded verification artifact from frozen
  `PaymentRequired` plus `VerifyResponse` examples
- no live wallet
- no live facilitator dependency
- no blockchain or testnet dependency
- no cloud service dependency

### 12.2 Hard fallback rule

If a real local generator would require:

- wallet setup
- funded test accounts
- live facilitator connectivity
- chain settlement
- flaky timing around expiry or confirmation
- demo infrastructure heavy enough to overshadow the seam

then the sample should fall back to a **docs-backed frozen artifact shape**.

That fallback is especially appropriate here because the goal is to isolate the
smallest honest external-consumer seam, not to recreate a full payment stack in
this repo.

## 13. Valid, invalid, malformed corpus

The first sample should follow the established corpus pattern.

### 13.1 Valid

One successful verification artifact with:

- one bounded resource reference
- one chosen scheme / network pair
- one bounded amount / asset pair
- `verification_result=verified`

### 13.2 Invalid

One rejected verification artifact with:

- one bounded resource reference
- one chosen scheme / network pair
- one bounded amount / asset pair
- `verification_result=rejected`
- one short `invalid_reason`

This is not a settlement failure record and not a merchant denial record. It is
only a bounded verification failure artifact.

### 13.3 Malformed

One malformed artifact that fails fast, for example:

- missing `scheme`
- missing `verification_result`
- unsupported `transport`
- missing `asset_ref`

## 14. Outward strategy

Do not open an outward x402 issue until the sample is on `main`.

Public x402 docs currently mention GitHub Discussion or Issue as support
routes. Even so, the preferred outward path for this lane should still be
**issue-first** unless upstream maintainers explicitly point elsewhere.

After that:

- one small GitHub issue
- one link
- one boundary question
- no broad payments pitch
- no settlement pitch
- no marketplace pitch

Suggested outward question:

> If an external evidence consumer wants the smallest honest x402 surface, is a
> bounded requirement-and-verification artifact derived from `PaymentRequired`
> plus `VerifyResponse` roughly the right place to start, or is there a thinner
> verification surface you would rather point them at?

## 15. Sequencing rule

This lane should still respect the current one-lane-at-a-time discipline.

Meaning:

1. let the freshest Browser Use, Visa TAP, Langfuse, and Mastra lanes breathe
   unless a maintainer responds
2. formalize `P15` now
3. build the `P15` sample only if no hotter follow-up overrides it
4. open the x402 issue only after the sample lands on `main`
5. treat `MCP` and `A2A` x402 transport expansion as later follow-up lanes, not
   part of the first sample

## 16. Non-goals

This plan does not:

- define a settlement-response evidence contract
- define a transaction-receipt evidence contract
- define a payer-identity evidence contract
- define merchant fulfillment correctness as Assay truth
- define diagnostic-extension semantics as Assay truth
- define `payment-identifier` as a first-class v1 lane

## References

- [x402 foundation repo](https://github.com/x402-foundation/x402)
- [x402 protocol specification v2](https://github.com/x402-foundation/x402/blob/main/specs/x402-specification-v2.md)
- [x402 HTTP 402 docs](https://github.com/x402-foundation/x402/blob/main/docs/core-concepts/http-402.md)
- [x402 facilitator docs](https://github.com/x402-foundation/x402/blob/main/docs/core-concepts/facilitator.md)
- [x402 HTTP transport v2](https://github.com/x402-foundation/x402/blob/main/specs/transports-v2/http.md)
- [x402 MCP transport v2](https://github.com/x402-foundation/x402/blob/main/specs/transports-v2/mcp.md)
- [x402 A2A transport v2](https://github.com/x402-foundation/x402/blob/main/specs/transports-v2/a2a.md)
- [x402 facilitator types](https://github.com/x402-foundation/x402/blob/main/typescript/packages/core/src/types/facilitator.ts)
- [x402 payment-identifier extension](https://github.com/x402-foundation/x402/blob/main/specs/extensions/payment_identifier.md)
- [x402 FAQ](https://github.com/x402-foundation/x402/blob/main/docs/faq.md)
- [RFC: Diagnostic extension for 402 responses](https://github.com/x402-foundation/x402/issues/1860)
- [Issue: irreversible server-side operations](https://github.com/x402-foundation/x402/issues/1886)
