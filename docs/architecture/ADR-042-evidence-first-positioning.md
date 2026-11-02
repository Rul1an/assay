# ADR-042: Evidence-first positioning and scope freeze

- Status: Accepted
- Date: 2026-07-24
- Supersedes: none
- Amended by: [ADR-043](ADR-043-evidence-chain-integrity-invariants.md) (accepted and implemented
  via #1842, #1843, #1850, #1851, #1852, and #1854) - states the integrity invariants this
  decision implies, without changing the scope or the stop list

## Context

Assay's distinguishing property is not breadth. It is bounded, recomputable evidence with explicit
claim ceilings: a record states what it can carry, and integrity never upgrades meaning. Adjacent
tools cover broad agent governance well, and several of them are explicit that their audit logs
record attempts and decisions rather than externally verified outcomes. The useful boundary for
Assay is the evidence question that begins there.

Without a written scope, that boundary erodes in the usual way: a score is added "for convenience",
a verdict starts summarising things it cannot observe, and the claims stop being trustworthy
precisely because they became broader.

## Decision

1. Assay is positioned evidence-first. The open profile for privileged MCP tool actions is the
   contract. The enforcing proxy is the reference producer of that evidence, not the contract
   itself, so independent producers and independent verifiers remain possible.

2. In scope: MCP `tools/call`; explicitly classified privileged actions; the pre-call policy
   decision; the caller-visible allow or deny outcome; optional post-action or boundary observation;
   source class, coverage, digests and non-claims; offline replay and CI or SARIF projection.

3. Out of scope, and refused by design (the stop list):
   - No aggregate trust score, safety rating or cleanliness scalar of any kind.
   - No whole-action verdict that collapses decision, observation and outcome into a single claim.
   - No detector catalogue or heuristic content scanning: model reasoning, prompt safety and
     content moderation are out.
   - No generic agent identity, delegation or policy federation. Compose with the established
     systems for those rather than reimplement them.
   - No provider-side outcome verification. An allow is the decision to forward, never proof that
     the action happened.
   - No compliance or "safe agent" claims.
   - No broad MCP scanning surface.

4. The profile is published as an open profile. The word "standard" is not used for it until an
   independent, non-author implementation reproduces the verifier from the specification text and a
   neutral venue has actually taken it up.

## Consequences

- Claims stay bounded and therefore checkable by someone who does not trust us.
- Work that does not strengthen the one evidence chain goes to HOLD rather than into the product.
- Kernel observation, protocol adapters and packs are supporting capabilities, not co-equal product
  directions.
- The stop list is normative. Introducing any item on it requires an ADR that supersedes this one,
  so the refusal cannot erode by accretion.

## Note (2026-11-02)

ADR-042's in-scope surface is MCP `tools/call`, the pre-call policy decision, the caller-visible
allow or deny, and optional post-action or boundary observation. A server ask that arrives while a
`tools/call` is in flight, and the client bytes that answer it, are boundary observations of frames
wrap already forwards; they are not a new privileged-action class, not a second decision, and not
proof that a human approved. Recording their original bytes beside the call's existing wire
identifiers (JSON-RPC id on a pending call or on an `input_required` result; `requestState` /
`inputRequests` keys when those members are present and well-typed) stays inside that
optional-observation clause. Attaching an unlinked `sampling/createMessage` or elicitation to a call
wrap did not observe, treating accept/decline as policy outcomes, or collapsing ask + decision +
server result into one verdict, would leave that scope and would need a superseding ADR.
