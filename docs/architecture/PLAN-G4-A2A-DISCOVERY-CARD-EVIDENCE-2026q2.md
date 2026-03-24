# PLAN — G4 A2A Discovery / Card Evidence Signal (2026 Q2)

- **Status:** Planned
- **Date:** 2026-03-24
- **Owner:** Evidence / Product

This wave starts with a **discovery gate** (Phase 0). **Phase 1 must not start until Phase 0 discovery outputs are reviewed and accepted.**

## North star

**G4 is an evidence-wave, not a pack-wave.** After [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md), the bottleneck for richer A2A claims is **evidence-shape** (what [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) can emit as first-class, typed canonical evidence), not pack-engine logic.

## Goal (one sentence)

Add **first-class canonical evidence** for A2A **discovery** and **Agent Card** surfaces, starting from **shipped adapter-emitted** reality rather than protocol aspiration; keep claims **bounded** to visibility/presence unless stronger **observed** evidence exists.

## Why G4 now

[P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) already proves **presence-only** companion-pack rules on `assay.adapter.a2a.*` for capabilities, task lifecycle, and artifact exchange. It deliberately does **not** claim authorization validity, signed Agent Card provenance, G3 parity, or discovery integrity — because those shapes are not first-class in typed payloads today.

G4 shifts the boundary by **adding evidence signals** (typed, bounded) so a later pack slice (**P2c**) can productize more than `event_type_exists` presence.

## Product framing

### In scope

- A2A-native **discovery/card** evidence signals where the adapter can support them honestly.
- **Typed, bounded, canonical** fields or one small subobject on emitted evidence.
- **Adapter-first** mapping and payloads; **evidence-first** contract discipline.
- **No validity theater** — visibility and observed facts before trust or verification claims.

### Out of scope

- Agent Card **verification engine** or full cryptographic provenance story in G4 v1.
- **Full A2A trustworthiness**, protocol compliance certification, or broad identity assurance.
- **Cryptographic completeness** or **temporal correctness** claims the runtime does not observe.
- A **new companion pack** in the G4 wave (deferred to **P2c** after G4 evidence exists).
- Pack **engine** version bump unless discovery proves a new check type is strictly necessary.

## External context (second-order)

The broader A2A ecosystem and security research motivate **why** discovery and Agent Card surfaces matter (interoperability, authenticated extended cards, signing discussions, spoofing analyses, implementation-oriented identity guidance). Those sources inform **motivation and non-goals**, not Assay’s v1 scope.

**Rule:** External sources motivate why the discovery/card area matters, but **they do not define G4 v1 scope**. **G4 scope is determined by shipped adapter-emitted evidence and the Phase 0 discovery matrices.**

## Phase 0 — Discovery freeze (gate)

Phase 0 must produce all of the following before any Phase 1 signal freeze:

1. Which A2A discovery/card-related signals already appear in adapter input, mapping, or `attributes`.
2. Which of those are **stable enough** to become **typed canonical** fields (vs remain unstructured).
3. Which are only honest as **presence** or **visibility** signals (not correctness).
4. Which candidates are **explicitly not** in G4 v1.
5. Whether **spec vs adapter** tension exists for the current **`>=0.2 <1.0`** support line (see [PLAN-P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — the section **“Adapter & protocol version reality (0.x)”**; link to the document, not a fragile anchor).

### Matrix A — Candidate triage

| Candidate signal (working name) | Upstream / source today | First-class in canonical evidence today? | Typed without overclaim? | G4 v1 (yes/no) |
|---------------------------------|-------------------------|------------------------------------------|---------------------------|----------------|
| *(fill during discovery)* | | | | |

### Matrix B — Field properties

| Field (working name) | Type (intended) | Redaction needed? | Stability risk | Bounded meaning (one line) |
|----------------------|-------------------|-------------------|------------------|----------------------------|
| *(fill during discovery)* | | | | |

## Phase 1 — Signal freeze

After Phase 0 is **reviewed and accepted**, Phase 1 freezes **either**:

- a small set of **2–4 typed fields**, **or**
- **one** small typed subobject,

**but not both** in G4 v1 unless discovery shows that the smaller shape alone would be **misleading**.

Concrete field names in this PLAN are **hypotheses** until Phase 0 completes.

## Hypothesis buckets for discovery (not frozen deliverables)

The following are **research categories** for Phase 0 — **not** committed deliverables:

1. **Agent Card discovery visibility** — e.g. card identifier/source visibility, capability source, basic vs extended discovery surface visibility.
2. **Extended-card access visibility** — only if the adapter can observe it: flows where authenticated extended card access occurred; **not** “auth valid” or “client trusted.”
3. **Signature material visibility** — only if upstream delivers it: signed-card presence, signature blob/metadata presence, verification attempted; **not** “signature valid” or “signer trusted.”
4. **Handoff / discovery-route visibility** — only if a **typed, narrower** field exists; **not** inferred from generic task metadata alone (same discipline as P2b).

Each bucket needs explicit **may imply** vs **must not imply** wording before any pack rule references it.

## Explicitly out of G4 v1

| Topic | Why excluded (v1) |
|-------|-------------------|
| Full card **verification** claim | No verification engine in G4 v1 |
| **Issuer trust** / chain integrity | Not observable as bounded evidence |
| **“Trusted Agent Card”** product language | Theater without signals |
| Full **discovery integrity** | Out of scope for v1 |
| **G3-auth clone for A2A** | Different protocol surface; no `assay.tool.decision` reuse theater |
| **New pack** in the G4 wave | **P2c** is downstream |
| **Engine bump** | Avoid unless strictly necessary |
| Broad **“A2A v1.0” protocol coverage** marketing | Adapter remains on the **`>=0.2 <1.0`** line per PLAN-P2b (*Adapter & protocol version reality* section) |

## Design rules

1. **Adapter reality first** — primary truth is emitted canonical evidence from [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/); the external spec informs non-goals and candidates, not automatic scope.
2. **Typed beats `attributes`** — free-form blobs are discovery input, not automatic product contract.
3. **Presence beats correctness** — prefer “visible / observed” before “valid / trusted.”
4. **One seam, one meaning** — do not mix observation, validation, trust score, and provenance claims in a single field.
5. **No adapter-reality inflation** — G4 planning and implementation must not turn **hints** or loose **`attributes`** into implied first-class coverage without Phase 0/1 freeze.

## Implementation expectation (future waves — not this PLAN PR)

**G4 implementation is expected to start in [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/)** (mapping, payload, conversion). **[`assay-evidence`](../../crates/assay-evidence/)** changes are **secondary** and only justified if a new **bounded classification seam** becomes necessary.

## Tests (future implementation)

When implementing G4 signals: emitted payload tests, typed-field presence tests, redaction tests, **no-overclaim** tests, version-gate tests, fixture-based adapter tests.

## Acceptance criteria (G4 “done”)

G4 implementation is complete when:

1. At least one A2A discovery/card surface is **first-class typed** in **canonical emitted** evidence (not only loose JSON blobs).
2. **Bounded meaning** is documented (what it implies vs what it does **not** prove).
3. Tests show unstructured blobs are **not** silently promoted to stronger claims.
4. Docs state explicit **non-proofs** (no issuer trust, no full verification, etc.).
5. **P2c** (follow-up pack) becomes honestly possible **because** G4 evidence exists — not before.
6. At least one new discovery/card signal appears in **emitted canonical evidence** with a **representative JSON example** in docs or tests (reviewable).

## P2c — follow-on (not G4)

**P2c — A2A Discovery / Card Follow-Up Pack** productizes **lint/pack rules** *after* G4 evidence ships — e.g. visibility rules aligned to G4 signals. **No pack YAML in this PLAN.** P2c must not be shipped in the same wave as G4 evidence implementation unless explicitly replanned.

## Reviewer checks (suggested)

- PLAN-G4 does **not** promise **full A2A v1.0 coverage** beyond current **shipped adapter** reality (`SUPPORTED_SPEC_VERSION_RANGE` / version gate).
- Hypothesis buckets are labeled as such in reviews; ROADMAP/RFC sequencing does not treat them as frozen scope.

## References

- [PLAN-P2b — A2A Signal Follow-Up Claim Pack](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) (P2b boundary; **Adapter & protocol version reality** section for 0.x line).
- [RFC-005 — Trust compiler MVP](RFC-005-trust-compiler-mvp-2026q2.md) §6 sequencing.
- [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — if trust or evidence contract floors change.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — engine bump discipline.
- [ROADMAP](../ROADMAP.md) — sequencing (G4 before P2c).
