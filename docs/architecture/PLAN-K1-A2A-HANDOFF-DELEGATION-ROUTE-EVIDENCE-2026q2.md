# PLAN — K1 A2A Handoff / Delegation-Route Evidence (2026 Q2)

- **Current status:** Next formal trust-compiler wave after `P2c`; [`K1-A` freeze path](./K1-A-PHASE1-FREEZE.md) and the first bounded adapter implementation slice are now merged on `main` and publicly released in **`v3.4.0`**; no pack or trust-artifact follow-up shipped in this slice.
- **Date:** 2026-03-27
- **Owner:** Evidence / Product
- **Inputs:** [DISCOVERY — Next Evidence Wave](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md), [Trust Compiler Audit Matrix](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md), [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md), [ROADMAP](../ROADMAP.md), [PLAN-G4](./PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md), [PLAN-P2c](./PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md)
- **Scope (this PR):** Formalize the next wave choice only. No adapter code, no pack YAML, no engine work, no Trust Basis or Trust Card changes.

## 1. Why this plan exists

The trust-compiler line on `main` is now shipped through `P2c`. The next step is no longer a vague
"post-`P2c` decision point": maintainers have chosen to formalize the preferred candidate from the
discovery note as a named wave.

That choice is:

- **`K1` — A2A handoff / delegation-route visibility evidence**

`K1` is a new wave label for bounded evidence work after the `G` / `P` trust-compiler line. It
does **not** imply a pack wave or a new trust-claim surface by itself.

`K1` is intentionally:

- **not** `P2d`
- **not** "more `G4`"
- **not** MCP authorization-discovery

The bottleneck is still **evidence shape**, not pack productization. So the next wave stays
**adapter-first**, **bounded**, and **visibility-only**.

## 2. Goal (one sentence)

Add a **first-class, bounded canonical evidence seam** for A2A **handoff / delegation-route
visibility**, starting from shipped adapter-emitted reality and staying strictly below correctness,
trust, or outcome claims.

In this plan, **"handoff / delegation-route"** refers to **one bounded evidence surface**, not
multiple independent seams.

## 3. Why `K1` now

### 3.1 What `P2c` did and did not close

`P2c` productized the **discovery/card** line that `G4-A` made visible. It did **not** establish a
typed seam for **handoff / delegation-route** evidence.

### 3.2 Why this is the strongest next bounded surface

The discovery note and audit matrix converge on the same gap:

- the A2A adapter already has discovery/card visibility
- the next missing surface is **agent-to-agent route / handoff visibility**
- that gap is more about **what canonical evidence exists** than about pack rules

MCP authorization-discovery remains relevant, but the stronger immediate bottleneck in Assay's
shipped evidence line is the A2A route / handoff surface.

### 3.3 Why this is not "another pack first"

No further A2A pack slice should be treated as default until Assay can observe a bounded,
first-class route / handoff seam honestly.

## 4. Product framing

### In scope

- A2A **handoff / delegation-route** as a bounded evidence topic
- Adapter-first work in [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) once freeze answers are ready
- One **small typed seam** in canonical emitted evidence
- **Presence / visibility** semantics only
- Explicit drop rules for ambiguous or blob-like upstream hints

### Out of scope

- Any **new pack** in the `K1` wave
- Any **engine bump**
- Any **Trust Basis** or **Trust Card** row in the same slice unless a later bounded predicate is separately justified
- Handoff **success**, delegation **correctness**, route **authorization**, chain **completeness**, temporal **correctness**, or provenance **trust**
- Reopening `G4-A` semantics or extending `payload.discovery` by default
- Folding in **MCP authorization-discovery**; that remains the second candidate, not part of `K1`

## 5. Hard language contract

`K1` may only claim that route / handoff information is **visible** in bounded evidence.

`K1` must **not** imply:

- handoff verified
- delegation succeeded
- delegation was allowed
- the chosen route was correct
- the target agent was trusted
- the full delegation chain is complete
- temporal or cryptographic validity

## 6. Working v1 seam hypothesis

The likely outcome is **one small typed subobject** or equivalent bounded field set in canonical A2A
payloads. Exact field names are **not frozen in this PLAN**.

This PLAN only freezes the shape discipline:

- one seam
- one meaning
- typed beats blob
- visibility beats correctness

Illustrative questions the seam may answer later:

- is a handoff / delegation-route surface visible at all?
- did the signal come from typed payload, an allowlisted extension path, or a lossy fallback?
- is there a bounded target / route kind visibility signal?

These are examples of the *kind* of seam `K1` may freeze later, not a field contract. They are
illustrative discovery directions, not provisional field commitments.

## 7. Phase 0 discovery freeze requirements

Before any implementation PR, `K1` must produce a freeze-ready discovery record that answers:

1. Which upstream A2A flows or fields actually carry route / handoff information?
2. Which are **typed and stable** enough for canonical promotion?
3. Which are only honest as **visibility** or **lossiness** signals?
4. What is the **smallest honest seam**?
5. Which upstream hints must **not** become typed route evidence?

Discovery answers must be grounded in shipped adapter-emitted evidence and current code-path
reality, not solely in protocol or standards text.

Minimum artifacts for the later freeze:

- per-field source mapping table
- precedence rules when multiple inputs exist
- representative emitted JSON example
- negative matrix for false positives
- explicit may / must-not-imply language

That freeze path now exists as [K1-A — Phase 1 formal freeze](./K1-A-PHASE1-FREEZE.md). It records the first executable contract for a bounded typed `handoff` seam while keeping `K1` itself evidence-first and implementation-free at the plan level.

## 8. Implementation gates (future, not this PR)

Any future `K1` implementation slice should hard-fail review if it:

- turns route visibility into route correctness
- guesses from generic task metadata without a frozen source rule
- promotes arbitrary blob content into typed route evidence
- silently reuses `payload.discovery` for a distinct route surface without an explicit freeze decision
- sneaks in a pack, engine bump, or trust-claim expansion in the same wave
- widens the seam beyond one bounded route / handoff surface

## 9. Acceptance for `K1-A`

`K1-A` should only count as shipped if all of the following hold:

1. Canonical emitted A2A evidence gains one bounded route / handoff seam.
2. The seam is documented with exact source paths and bounded meaning.
3. Tests show the seam is **not** promoted from loose or ambiguous input.
4. Product language stays at **visible / observed**, not **valid / trusted**.
5. No pack or broader trust artifact is shipped in the same wave.

Current status on `main`: these conditions are now met for the first bounded A2A adapter seam
shipped in `assay-adapter-a2a`. `K1-A` remains visibility-only and still has **no** downstream pack
or broader trust-artifact follow-up in the same wave.

## 10. What happens after `K1`

Only after `K1` evidence is real should maintainers revisit whether:

- a future A2A follow-up pack is honest
- MCP authorization-discovery should become the next evidence-wave
- any Trust Basis / Trust Card expansion is justified

No downstream pack or trust-surface follow-up should be assumed as part of `K1` closure.

`K1` is therefore the next **formal evidence wave**, not the start of an automatic new pack line.

## References

- [DISCOVERY — Next Evidence Wave](./DISCOVERY-NEXT-EVIDENCE-WAVE-2026Q2.md)
- [Trust Compiler Audit Matrix](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md)
- [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)
- [K1-A — Phase 1 formal freeze](./K1-A-PHASE1-FREEZE.md)
- [PLAN-G4](./PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md)
- [PLAN-P2c](./PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md)
