# DISCOVERY — Next Evidence Wave (2026 Q2)

- **Status:** Historical discovery note; preferred candidate is now formalized as [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md), and the first bounded `K1-A` adapter slice is merged on `main`
- **Date:** 2026-03-25
- **Owner:** Evidence / Product
- **Scope (this document now):** Historical ranking input only. The formal next-wave choice now lives in [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md).

## 1. Decision frame

This note originally did **not** formalize the next wave. It recorded the candidate surfaces after [G4-A Phase 1](G4-A-PHASE1-FREEZE.md) and [P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md) and applied the same discipline as earlier waves: **evidence reality first**, productization later.

That choice has now been made: the preferred candidate below is formalized as [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md). This note remains the discovery record that justified that choice.

### Two-question filter

| Question | If **yes** | If **no** |
|----------|------------|-----------|
| Does **bounded, first-class** evidence for this surface already exist in bundles? | Go to question 2 | **Evidence-wave first**; no pack yet |
| Can a **small, honest, product-useful** claimset be built on top (no theater)? | A **P-slice may be justified** — not automatic; still needs product scope and maintainer agreement | **No wave yet** — tighten product scope or wait |

**Working implication:** A new pack is **not** justified without both evidence reality **and** a bounded claimset. **Yes / yes** is **plausible** for a future P-slice, not a mandate to ship one.

### Why no ROADMAP change in this PR

This note is **discovery-only** and does **not** commit maintainers to the next formal wave; updating [ROADMAP](../ROADMAP.md) here would imply a strategic decision that belongs **after** explicit maintainer choice.

## 2. North star

The next meaningful step after P2c is **not automatically another pack**. It is whichever surface has the strongest case under the filter above.

Assay should continue to prefer:

- **Adapter-emitted, bounded** seams
- **Typed / canonical** evidence over blob interpretation
- **Presence / visibility** claims over validity or trust claims
- **One surface at a time**
- **New protocol topic ≠ automatic pack** — evidence reality and an honest claimset come first (same seam ≠ same pack discipline)

[G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md) remains the reference pattern for how a **later** freeze should look **once** a surface is chosen and implemented.

## 3. Repo reality at time of discovery

From the current **A2A** adapter implementation:

- **Canonical event mapping** is in [`crates/assay-adapter-a2a/src/adapter_impl/mapping.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/mapping.rs): `agent.capabilities`, `task.requested`, `task.updated`, `artifact.shared`; unknown upstream types map to a generic path (`assay.adapter.a2a.message` — see [`convert.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/convert.rs)).
- **At the time of discovery**, first-class A2A surfaces in shipped evidence were primarily **capabilities**, **task lifecycle**, **artifact exchange**, and (after G4-A) **discovery / card visibility** via `payload.discovery` ([`discovery.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/discovery.rs)).
- **At the time of discovery**, there was **not** yet a **first-class, bounded handoff / delegation-route** seam in adapter output (no dedicated mapping or typed subobject for that route in the codepath then under review).
- **G3** applies to **`assay.tool.decision`** authorization **context** ([`g3_authorization_context.rs`](../../crates/assay-evidence/src/g3_authorization_context.rs)) — **not** MCP authorization-**discovery** / resource-metadata as a separate bundle seam (see [§ Candidate 2](#5-candidate-2--mcp-authorization-discovery)).

These repo facts were the **primary** basis for the candidate ranking below. They are now historical context; `K1-A` Phase 1 has since added the first bounded top-level `handoff` seam on `main`.

## 4. Candidate 1 — A2A handoff / delegation-route visibility

**Status:** **Preferred** candidate for the next **evidence** wave (not a pack wave).

### Why this surface matters

A2A’s public framing emphasizes agent-to-agent collaboration, task management, and interoperability. Recent community discussion also highlights ambiguity around **transfer / handoff** semantics — a signal that protocol value can exist **before** clean evidence seams do.

### Why this is not “more G4” or “more P2c”

This is a **different surface** from discovery/card visibility. It must **not** be treated as:

- an automatic extension of `payload.discovery`
- an automatic extension of [P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md)
- a reason to append rules onto existing discovery/card packs (“same seam ≠ same pack”)

### Current gap

At the time of discovery, the adapter’s mapped evidence did **not** expose a bounded, typed **handoff / delegation-route** seam. That bottleneck is why the path chosen here was **evidence-shape first** (adapter-first), not pack productization.

### Likely outcome under the filter

| Q1 (evidence exists?) | Q2 (honest claimset?) | Provisional result |
|-----------------------|------------------------|-------------------|
| Probably **no** | N/A yet | **Evidence-wave first** |

### Discovery questions

- Which upstream A2A flows or fields carry handoff/delegation-route information?
- Which are **typed and stable** vs blob-like or lossy?
- What is the **minimum honest seam** (one small subobject, one bounded visibility set, etc.)?
- **Explicit non-goals:** no trust score, no provenance “success”, no correctness of delegation **outcome** as an observed claim unless evidence supports it.

## 5. Candidate 2 — MCP authorization-discovery

**Status:** **Second** candidate.

### Why this surface matters

The MCP authorization line requires **OAuth 2.0 Protected Resource Metadata** and **authorization-server discovery** as part of the protocol story — **authorization-discovery** is a real protocol surface, not a minor implementation detail.

**Product guard:** Even if the MCP spec makes this surface important, it is **not** a next-wave candidate **unless** canonical Assay evidence already exposes it or a **dedicated evidence-wave** is explicitly chosen. Spec relevance ≠ bundle reality.

### Critical contrast with G3

**G3 is not this surface.**

| G3 (shipped) | MCP authorization-discovery (candidate) |
|--------------|----------------------------------------|
| Authorization **context** on **`assay.tool.decision`** | Resource metadata / authorization-server **discovery** as its own observable seam |
| [G3 predicate](../../crates/assay-evidence/src/g3_authorization_context.rs) aligned with Trust Card / packs | Not implied by “we did auth on tool decisions” |

### Current gap

The spec can make the surface **important**; that does **not** prove Assay bundles already emit it as **bounded canonical evidence**.

### Likely outcome under the filter

| Q1 | Q2 | Provisional result |
|----|----|--------------------|
| Unknown / probably **partial** | Only if Q1 becomes **yes** | Likely **evidence-wave first** unless an audit finds the seam already first-class |

### Discovery questions

- Where would PRM / AS-discovery appear in current Assay bundles (event types, payloads)?
- Is it already first-class and typed, or only implied by spec-level expectations?
- Could a bounded claimset exist without inventing new semantics?

## 6. Candidate 3 — Trust / signed-card provenance

**Status:** **Not a candidate for the next wave.**

Public discussion and research emphasize signing, verification, and spoofing risk — **visibility** evidence (as in G4-A / P2c) can be honest; **trust / provenance** claims without bounded signals are high-risk theater.

**Conclusion:** Park until there is bounded first-class evidence, a non-theatrical claimset, and alignment with **G4 / P2c non-goals** (no reopening signature semantics as product claims).

## 7. External context (non-normative)

This section is **second-order** motivation only. It does **not** define Assay scope.

- **A2A** positions discovery, collaborative tasks, and secure collaboration as core protocol surfaces; official samples stress treating external agent data (including Agent Cards and task/artifact content) as **untrusted input** where applicable.
- **A2ASecBench**-style work highlights discovery/card spoofing and related protocol attacks as practical risks — supporting **careful visibility evidence**, not premature trust marketing.
- **NIST** / ecosystem work (e.g. agent identity, authorization, auditing) favors **observable controls** over marketing-style “trust” claims without evidence.
- **MCP** authorization work makes **authorization-discovery** (Protected Resource Metadata, authorization server locations) a protocol-level requirement in current spec direction.

**Hard rule:** External sources explain **why** surfaces matter; **bundle evidence reality** in this repo determines **what** Assay can honestly ship.

## 8. Provisional decision

| Role | Surface |
|------|---------|
| **Preferred next evidence-wave candidate** | A2A handoff / delegation-route visibility |
| **Second candidate** | MCP authorization-discovery evidence (especially if enterprise/IAM priority rises) |
| **Explicitly not next** | Trust / signed-card provenance |

## 9. Next steps

1. **Done:** Candidate 1 is now formalized as [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md).
2. **Done in this slice:** the next execution artifact is now recorded as [K1-A — Phase 1 formal freeze](./K1-A-PHASE1-FREEZE.md), with explicit source mapping, emitted examples, and no implementation in the freeze slice itself.
3. **Done:** the first bounded adapter-first implementation slice for `K1-A` is now merged on `main` as the top-level A2A `handoff` seam, still without a pack wave.
4. Any further `K1` widening remains a separate maintainer decision; no downstream pack follows automatically from this slice.
5. This note remains the historical discovery input; it no longer carries the active roadmap choice by itself.

## References (repo)

- [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md)
- [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md)
- [PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md)
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — sequencing; review gates for future execution
- [`mapping.rs` (A2A event mapping)](../../crates/assay-adapter-a2a/src/adapter_impl/mapping.rs)
- [`g3_authorization_context.rs`](../../crates/assay-evidence/src/g3_authorization_context.rs)
