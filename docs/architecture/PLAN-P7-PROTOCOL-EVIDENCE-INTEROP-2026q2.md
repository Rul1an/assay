# PLAN — P7 Protocol Evidence Interop Track (2026 Q2)

- **Date:** 2026-04-07
- **Owner:** Evidence / Product
- **Status:** Planning and execution kickoff
- **Scope (this PR):** Formalize the external protocol track after the framework wave. No adapter changes, no pack changes, no protocol claims, no external posting in this slice.

## 1. Why this plan exists

Point 7 is not a clean-sheet protocol exploration.

Assay already has real internal protocol substrate for:

- **A2A** via [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/src/lib.rs)
- **UCP** via [`assay-adapter-ucp`](../../crates/assay-adapter-ucp/src/lib.rs)
- related trust-compiler follow-on work around A2A discovery cards, handoff visibility, and UCP adapter scope in:
  - [PLAN-ADR-026-A2A-2026q2.md](./PLAN-ADR-026-A2A-2026q2.md)
  - [PLAN-ADR-026-UCP-2026q2.md](./PLAN-ADR-026-UCP-2026q2.md)
  - [PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md)

So the next honest move is **not** “add protocol support.” The next honest move is:

1. identify the smallest external-consumer seam for each protocol track,
2. prove that seam with one tiny Assay-side sample,
3. ask one narrow upstream question in the channel that best fits that protocol community.

This plan covers that execution line for:

- **P7-A:** A2A
- **P7-B:** UCP
- **P7-C:** agent-payments / commerce-adjacent protocol watchlist

## 2. Hard rules for the protocol track

Every protocol lane in `P7` follows the same boundaries:

- Assay is positioned as an **evidence compiler / external evidence consumer**, not as another protocol authority.
- We consume **bounded observed protocol artifacts**, not protocol truth.
- We do **not** inherit trust scores, policy meaning, correctness, routing correctness, or economic legitimacy as Assay truth.
- We build **one seam at a time**.
- We do **not** open with a broad protocol pitch.
- We do **not** post upstream until the sample is merged on `main`.

Common anti-overclaim sentence for all outward posts:

> We are not asking Assay to inherit protocol semantics, trust semantics, payment legitimacy, or runtime judgments as truth.

## 3. Channel strategy by protocol

The protocol track does **not** reuse the same outreach shape everywhere.

### A2A

- Upstream repo: [a2aproject/A2A](https://github.com/a2aproject/A2A)
- Current upstream shape: active protocol and trust/accountability discussion happens in **issues**
- Relevant signal:
  - [issue #1718](https://github.com/a2aproject/A2A/issues/1718) shows active interest in trust/accountability primitives
- Best route: **sample first, then one small issue**

### UCP

- Upstream repo: [Universal-Commerce-Protocol/ucp](https://github.com/Universal-Commerce-Protocol/ucp)
- Current upstream shape:
  - Discussions are enabled
  - the discussion feed visibly carries roadmap, guidance, and community show-and-tell
- Best route: **sample first, then one small discussion / show-and-tell**

### Agent-payments watchlist

- Current shape: fragmented
- No single Assay-target protocol is clearly stable enough yet to justify build-first work
- Best route: **watchlist first, not implementation first**

## 4. Protocol ranking inside point 7

### 4.1 First: A2A

Why first:

- Assay already has the deepest internal substrate here
- the upstream repo is active and explicitly engaged with trust/accountability questions
- we can ask a narrow seam question without pretending to solve A2A trust wholesale

### 4.2 Second: UCP

Why second:

- Assay also has internal substrate here
- UCP has a cleaner show-and-tell lane than A2A
- the commerce semantics are richer, so we should lead with one small lifecycle seam, not a bigger “agent payments” story

### 4.3 Third: agent-payments / adjacent protocols

Why third:

- the field is still fragmented
- the risk of overclaiming is higher
- we should let A2A and UCP teach us which protocol-facing sample shape works best before we generalize

## 5. P7-A — A2A execution plan

### 5.1 Goal

Build one tiny A2A external-evidence sample that Assay can point to in the A2A repo without reopening “what is trust in A2A” as a general argument.

### 5.2 Proposed first seam

Use a **task-lifecycle-plus-route-reference** seam as the first external sample.

That means:

- one bounded task request artifact
- one bounded task update artifact
- one malformed artifact
- optional route / handoff reference only if it is already expressible without turning the sample into a second seam

This is intentionally narrower than:

- bilateral signed interaction records
- trust-score proposals
- agent-card trust profile claims
- capability-token / authz systems

### 5.3 Why this seam

This seam is the best first candidate because it:

- overlaps with Assay’s existing A2A adapter reality
- stays close to core protocol flows
- avoids hijacking the wider upstream trust/accountability proposals
- still leaves room later to ask whether route/handoff visibility or discovery-card visibility should become the preferred external-consumer seam

### 5.4 Concrete repo deliverable

Add:

- `examples/a2a-task-evidence/README.md`
- `examples/a2a-task-evidence/map_to_assay.py`
- `examples/a2a-task-evidence/fixtures/valid.a2a.ndjson`
- `examples/a2a-task-evidence/fixtures/failure.a2a.ndjson`
- `examples/a2a-task-evidence/fixtures/malformed.a2a.ndjson`
- `examples/a2a-task-evidence/fixtures/valid.assay.ndjson`
- `examples/a2a-task-evidence/fixtures/failure.assay.ndjson`

The sample must:

- keep upstream timestamps separate from Assay import time
- keep route / handoff reference optional and bounded
- keep discovery-card, identity, authz, and trust-profile semantics out of v1
- preserve malformed import behavior as an explicit failure case

### 5.5 README language contract

The README must say:

- this is not a production Assay↔A2A adapter
- this does not freeze a new Assay Evidence Contract event type
- this does not treat A2A task outcomes, delegation correctness, or trust/accountability semantics as Assay truth

### 5.6 Upstream outreach after merge

After the sample lands on `main`, open **one** small A2A issue.

That issue should:

- link only to the sample
- explain the seam in two short paragraphs
- ask one question only:

> For an external evidence consumer trying to stay small and honest, which A2A artifact or packet surface would you most want them to align to as the smallest stable seam?

### 5.7 What not to do on A2A

- do not post inside a broader trust-primitive thread like [#1718](https://github.com/a2aproject/A2A/issues/1718)
- do not present Assay as the trust layer for A2A
- do not lead with signed-record or reputation claims
- do not combine Agent Cards, task lifecycle, route evidence, and authz into one first sample

## 6. P7-B — UCP execution plan

### 6.1 Goal

Build one tiny UCP commerce-adjacent sample that shows Assay can consume bounded observed protocol state without claiming economic or payment truth.

### 6.2 Proposed first seam

Use a **checkout / order lifecycle** seam.

That means:

- one valid lifecycle artifact
- one failure or denied-progress artifact
- one malformed artifact

This is intentionally narrower than:

- catalog semantics as truth
- merchant legitimacy
- payment settlement truth
- fulfillment correctness

### 6.3 Why this seam

This seam is the right first UCP slice because it:

- stays inside the already-frozen UCP adapter MVP scope
- is recognizable to UCP maintainers
- keeps the story commerce-adjacent without overclaiming “agent payments”

### 6.4 Concrete repo deliverable

Add:

- `examples/ucp-checkout-evidence/README.md`
- `examples/ucp-checkout-evidence/map_to_assay.py`
- `examples/ucp-checkout-evidence/fixtures/valid.ucp.ndjson`
- `examples/ucp-checkout-evidence/fixtures/failure.ucp.ndjson`
- `examples/ucp-checkout-evidence/fixtures/malformed.ucp.ndjson`
- `examples/ucp-checkout-evidence/fixtures/valid.assay.ndjson`
- `examples/ucp-checkout-evidence/fixtures/failure.assay.ndjson`

The sample must:

- pin to the frozen UCP adapter version anchor already documented internally
- keep order / checkout state as observed metadata only
- avoid payment authorization / settlement semantics in v1

### 6.5 Upstream outreach after merge

After merge, open **one small UCP discussion** rather than an issue.

Why discussion:

- UCP already uses Discussions for guidance and community show-and-tell
- the first outward move here is better framed as “small sample / boundary question” than as a problem report

The one question should be:

> If an external evidence consumer wants the smallest honest UCP seam, would you rather point them at checkout / order lifecycle state, or at some thinner exported artifact surface?

### 6.6 What not to do on UCP

- do not call it a payment protocol integration in the first post
- do not imply merchant, checkout, or payment legitimacy
- do not lead with fulfillment, refunds, identity, or marketplace trust surfaces

## 7. P7-C — Agent-payments watchlist

### 7.1 Why watchlist instead of build now

The agent-payments space is strategically interesting, but still too fragmented for a clean Assay execution lane today.

The current risks are:

- protocol churn
- vendor-specific surfaces disguised as standards
- payment semantics that are much easier to overclaim than order / task semantics

### 7.2 Activation criteria

We only promote this from watchlist to active work if a candidate surface has:

- a public repo and maintained spec or protocol docs
- an obvious maintainer channel
- a bounded artifact or event surface
- at least one valid / invalid / malformed corpus we can honestly mirror
- a clear distinction between observed record and economic/legal truth

### 7.3 Practical implication

For now:

- **A2A** covers the protocol-accountability adjacency
- **UCP** covers the commerce / checkout adjacency
- explicit “agent payments protocol” work stays deferred

## 8. Recommended execution order

### Immediate

1. ship this plan doc
2. build `examples/a2a-task-evidence/`
3. merge it
4. post one A2A issue

### Next

5. build `examples/ucp-checkout-evidence/`
6. merge it
7. post one UCP discussion

### Later

8. re-evaluate agent-payments candidates using the watchlist criteria

## 9. Success criteria

`P7` is on track if:

- Assay has one merged A2A sample
- Assay has one merged UCP sample
- each upstream protocol gets exactly one small, protocol-appropriate post
- there is no semantic overreach in README, mapper, or outreach copy
- agent-payments remains watchlist-only unless a genuinely stable surface appears

## 10. Non-goals

- shipping new A2A or UCP adapter semantics in this plan
- creating new Trust Basis or Trust Card claims from these protocol tracks
- proposing protocol-level trust primitives upstream
- treating protocol outputs, order state, or payment-adjacent results as Assay truth

## References

- [PLAN-ADR-026-A2A-2026q2.md](./PLAN-ADR-026-A2A-2026q2.md)
- [PLAN-ADR-026-UCP-2026q2.md](./PLAN-ADR-026-UCP-2026q2.md)
- [PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md)
- [a2aproject/A2A](https://github.com/a2aproject/A2A)
- [Universal-Commerce-Protocol/ucp](https://github.com/Universal-Commerce-Protocol/ucp)
- [A2A issue #1718](https://github.com/a2aproject/A2A/issues/1718)
