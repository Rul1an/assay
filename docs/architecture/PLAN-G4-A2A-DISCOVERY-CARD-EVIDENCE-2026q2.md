# PLAN — G4 A2A Discovery / Card Evidence Signal (2026 Q2)

- **Status:** Phase 0 discovery **recorded** (pending **human review** before Phase 1 freeze)
- **Date:** 2026-03-24
- **Owner:** Evidence / Product
- **Phase 0 source snapshot:** `assay-adapter-a2a` as of PLAN update (see Matrix A/B + record below)

This PLAN defines **gates**, **hypotheses**, and **acceptance** for G4; it does **not** freeze final signal shapes by virtue of being written—those follow Phase 0/1 review.

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

Filled from **code inspection** of [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) (`mapping.rs`, `convert.rs`, `payload.rs`). No separate runtime telemetry was used for this pass.

| Candidate signal (working name) | Upstream / source today | First-class in canonical evidence today? | Typed without overclaim? | G4 v1 candidate (yes/no / support) |
|---------------------------------|-------------------------|------------------------------------------|---------------------------|-------------------------------------|
| **Agent capability identifiers** (`agent.capabilities`) | `agent.capabilities` string array; `event_type: agent.capabilities` → `assay.adapter.a2a.agent.capabilities` | **Yes** — `payload.agent.capabilities` (deterministic string array in emitted JSON) | **Yes** — “strings advertised in this event”; **not** auth validity or card proof | **Yes** — discovery-adjacent; **already shipped**; G4 v1 may add **docs + tests** for bounded meaning without new keys |
| **Agent id / name / role** | `agent.id`, `agent.name`, `agent.role` | **Yes** — `payload.agent.*` | **Yes** — observed identity strings | **Support** — general agent surface; not card-specific |
| **Protocol packet version** | `version` on packet; optional transport `protocol_version` | **Yes** — `protocol_version` in payload | **Yes** — observed protocol line | **Support** — version gate (`0.2+`) already enforced in code |
| **Upstream + canonical event routing** | `event_type` | **Yes** — `upstream_event_type` + CloudEvents-style `event.type` | **Yes** — which logical event fired | **Support** |
| **Extension `attributes` blob** | Top-level `attributes` object | **Yes** — normalized JSON in `payload.attributes` | **Passthrough only** — **no** Assay semantics for card/discovery | **No** — do **not** treat as card/discovery proof without Phase 1 key contract |
| **Agent Card URL / discovery document / signature blobs** | Not extracted in adapter | **No** — at best inside `attributes` or extra top-level keys | **No** — would be speculative | **No** for v1 unless Phase 1 freezes **explicit** upstream shapes to map |
| **Unmapped top-level field count** | Non-reserved keys at packet root | **Yes** — `unmapped_fields_count` | **Yes** — lossiness / “extra stuff present” | **Support** — not a card/discovery semantic |

### Matrix B — Field properties (existing typed surfaces)

| Field (working name) | Type (intended) | Redaction needed? | Stability risk | Bounded meaning (one line) |
|----------------------|-----------------|-------------------|----------------|----------------------------|
| `payload.agent.capabilities` | string[] | Unusual if entries embed secrets | Low for URI-like capability strings | Capability identifiers **observed on this packet**; not proof of authorization or card authenticity |
| `payload.agent.id` | string | If treated as PII in your tenant | Low | Agent id string from upstream |
| `payload.agent.name` / `.role` | string | If PII | Low | Optional display / role strings from upstream |
| `payload.attributes` | object | Often yes — copied verbatim | **High** (schema-less) | Opaque producer extension JSON — **no** implied Agent Card or discovery semantics |
| `payload.protocol_version` | string | Rare | Low | `version` field observed on the A2A packet |
| `payload.unmapped_fields_count` | number | No | Low | Count of top-level keys outside the adapter’s reserved set |

### Phase 0 discovery record (codebase pass)

1. **What appears today?** Typed extraction covers `protocol`, `version`, `event_type`, `timestamp`, `agent` (id, name, role, capabilities), `task`, `artifact`, `message`, and passthrough **`attributes`**. **No** dedicated fields for Agent Card URLs, discovery endpoints, signed card bodies, or extended-card auth flows — those could only appear inside **`attributes`** or as **unmapped** top-level keys (see `count_unmapped_top_level_fields` in `mapping.rs`).

2. **Stable enough for typed canonical?** **`agent.capabilities`** and other **`agent.*`** fields are **already** first-class in `build_payload`. **`attributes`** is stable as an **opaque blob**, not as card/discovery semantics.

3. **Presence / visibility only?** **`attributes`** content, **generic** `assay.adapter.a2a.message` fallback, and **unmapped** keys are **visibility / lossiness** signals unless Phase 1 assigns a schema.

4. **Explicitly not G4 v1 without new work:** Typed **Agent Card document**, **discovery URL**, **signature verification**, **issuer trust** — see [Explicitly out of G4 v1](#explicitly-out-of-g4-v1). **Promoting** arbitrary **`attributes`** keys to first-class card/discovery fields is **out** until a **Phase 1 freeze** names producer-stable paths.

5. **Spec vs adapter (`>=0.2 <1.0`)?** Capabilities advertise **`SUPPORTED_SPEC_VERSION_RANGE`** as `>=0.2 <1.0`. **Runtime validation** (`version.rs`) accepts **0.2+** (major `0`, minor ≥ `2`). That matches the P2b story: **shipped line is 0.x**, not a marketing claim of full **A2A v1.0** coverage. No code change required for this Phase 0 answer.

**Phase 0 gate:** Discovery matrices + record above are **complete for the adapter codebase snapshot**. **Phase 1 must not start** until Evidence/Product **reviews and accepts** this record (and any amendment to how [Acceptance criteria](#acceptance-criteria-g4-done) §1 / §6 apply — see below).

### Open decision — Phase 1 path (A or B)

The adapter **already** emits typed **`agent.capabilities`**. [Acceptance criteria](#acceptance-criteria-g4-done) ask for a discovery/card-related surface and “at least one **new** … signal.” Reviewers must **explicitly choose** one product path:

| | **Option A — new typed surface** | **Option B — bounded reuse (no new keys)** |
|---|----------------------------------|---------------------------------------------|
| **What ships** | **New** first-class field(s) or subobject in canonical emitted evidence (adapter mapping change). | **Hardening / clarification** on **existing** first-class `payload` — new **bounded meaning**, tests, examples; **no** new typed keys. |
| **Evidence-wave read** | Stronger fit for “G4 = evidence-wave” as **new** observable seam. | Legitimate, but G4 reads as **interpretation / documentation** on existing adapter output rather than a new evidence seam. |
| **Phase 1** | Map agreed upstream or `attributes` paths; adapter + tests. | Docs + tests + examples only; adapter unchanged unless fixes are needed for unrelated reasons. |

**(A)** Phase 1 adds **new** first-class field(s) mapped from agreed upstream/`attributes` shapes. **(B)** G4 v1 treats **capability + identity visibility** as the **discovery-adjacent** wave and satisfies acceptance via **new bounded-meaning documentation, tests, and examples** on **existing** fields — **only if** product accepts **B** and applies the **§1 / §6** rows for **Option B** in [Acceptance criteria](#acceptance-criteria-g4-done).

## Phase 1 — Signal freeze

After Phase 0 is **reviewed and accepted**, Phase 1 freezes **either**:

- a small set of **2–4 typed fields**, **or**
- **one** small typed subobject,

**but not both** in G4 v1 unless discovery shows that the smaller shape alone would be **misleading**.

**Phase 0 codebase pass:** complete (see matrices + record). **Phase 1 field names** remain **frozen by review**, not by this document alone.

**Provisional Phase 1 directions (hypotheses — not frozen):**

1. **Document + test** bounded meaning for **`payload.agent.capabilities`** (and identity fields) under the discovery/card narrative — **no overclaim** vs P2b / this PLAN.
2. **Only if** a real producer emits **stable** optional keys (e.g. in `attributes`), consider **promoting** named paths in a **separate** Phase 1 decision — with adapter tests and explicit non-proofs.
3. **Do not** invent Agent Card **verification** or **trust** claims; stay aligned with [Explicitly out of G4 v1](#explicitly-out-of-g4-v1).

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

**How to read §1 and §6 depends on the Phase 1 path ([Open decision — Phase 1 path (A or B)](#open-decision--phase-1-path-a-or-b)):**

| Criterion | **Option A** (new typed surface) | **Option B** (bounded reuse — no new adapter keys) |
|-----------|----------------------------------|-----------------------------------------------------|
| §1 | At least one **new** first-class discovery/card-adjacent field or subobject in **canonical emitted** evidence. | At least one **existing** first-class typed surface (e.g. `payload.agent.capabilities` and related `agent.*`) is **explicitly** designated as the discovery/card-adjacent evidence surface — still **typed** in payload, not loose blobs. |
| §6 | At least one **new** typed signal in emitted evidence, with **representative JSON** in docs or tests. | **New** **bounded-meaning** contract, tests, and **representative JSON** for the **existing** payload shape (the “new” deliverable is the **semantics + examples**, not new keys). |

Reviewers should **record A or B** when accepting Phase 0 so implementation is not ambiguous.

G4 implementation is complete when:

1. At least one A2A discovery/card surface is **first-class typed** in **canonical emitted** evidence (not only loose JSON blobs) — interpret per **A/B** table above.
2. **Bounded meaning** is documented (what it implies vs what it does **not** prove).
3. Tests show unstructured blobs are **not** silently promoted to stronger claims.
4. Docs state explicit **non-proofs** (no issuer trust, no full verification, etc.).
5. **P2c** (follow-up pack) becomes honestly possible **because** G4 evidence exists — not before.
6. Discovery/card work is **reviewable** via emitted canonical evidence: per **A/B**, either **new** typed signal(s) **or** **new** semantics + examples on **existing** typed fields — with **representative JSON** in docs or tests.

## P2c — follow-on (not G4)

**P2c — A2A Discovery / Card Follow-Up Pack** productizes **lint/pack rules** *after* G4 evidence ships — e.g. visibility rules aligned to G4 signals. **No pack YAML in this PLAN.** P2c must not be shipped in the same wave as G4 evidence implementation unless explicitly replanned.

## Reviewer checks (suggested)

### Reviewer focus (Phase 0 → Phase 1)

The review question is **not** “is G4 a good idea?” but:

1. Is Phase 0 **sufficiently grounded and honest** (matrices + spec-vs-adapter + no premature code)?
2. Which **Phase 1 path** applies — **A** (new typed surface) vs **B** (bounded reuse + docs/tests/examples on existing payload)?
3. Are [Acceptance criteria](#acceptance-criteria-g4-done) **correct for the chosen path** — especially if **B** (no new typed keys in G4 v1)?

### General checks

- PLAN-G4 does **not** promise **full A2A v1.0 coverage** beyond current **shipped adapter** reality (`SUPPORTED_SPEC_VERSION_RANGE` / version gate).
- Hypothesis buckets are labeled as such in reviews; ROADMAP/RFC sequencing does not treat them as frozen scope.

## References

- [PLAN-P2b — A2A Signal Follow-Up Claim Pack](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — P2b boundary; read the section **Adapter & protocol version reality (0.x)** in that document for the version gate.
- [RFC-005 — Trust compiler MVP](RFC-005-trust-compiler-mvp-2026q2.md) §6 sequencing.
- [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — SSOT for consumer/version floors if G4 implies contract or `requires` changes.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — consult before adding new pack check types in a follow-on **P2c** wave.
- [ROADMAP](../ROADMAP.md) — high-level sequencing only (G4 before P2c); hypotheses stay in this PLAN.
