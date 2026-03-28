# PLAN — G4 A2A Discovery / Card Evidence Signal (2026 Q2)

- **Current status:** **G4-A Phase 1** is merged on `main` and publicly released in **`v3.4.0`** ([G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md), [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) `payload.discovery`). Remaining work in this track is **post-merge verification / release-truth hygiene only** — no new G4 evidence semantics. **P2c** (A2A discovery/card follow-up pack) is also public in **`v3.4.0`** — see [§ P2c — follow-on (not G4)](#p2c-follow-on-not-g4).
- **Date:** 2026-03-24 (plan); Phase 1 merged 2026-03-24 (PR #944).
- **Owner:** Evidence / Product
- **Phase 0 source snapshot:** `assay-adapter-a2a` as of original PLAN update (see Matrix A/B + record below); Phase 1 signal shapes are frozen in [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md).

This PLAN defines **gates**, **hypotheses**, and historical **acceptance** for G4; normative Phase 1 contracts live in the freeze linked above.

**Historical gate:** Phase 0 discovery was reviewed before Phase 1; **G4-A** (Option A — new typed `discovery` seam) shipped per that freeze.

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

**Critical distinction (avoid over-reading Matrix A):** `payload.agent.capabilities` is **strong discovery-adjacent** typed evidence, but it is **not by itself** a first-class **Agent Card** or **discovery-document** seam. “Capabilities visible” ≠ “card/discovery surface fully covered in evidence.”

**Mechanical classification for review:**

| Bucket | Items |
|--------|--------|
| **Primary G4 v1 candidate (discovery-adjacent, already typed)** | **`agent.capabilities`** — only row tagged **Yes** for G4 v1 candidate in the matrix sense; any G4 v1 work may still add **docs/tests** without new keys (path **B**) or **new** fields (path **A**). |
| **Supporting context** | Agent id / name / role, `protocol_version`, upstream + canonical event routing, `unmapped_fields_count` — useful context, not card/discovery-specific seams. |
| **Explicit non-candidates (v1 without Phase 1 seam work)** | **`attributes`** as a discovery/card **signal**; **Agent Card URL / discovery document / signature blobs** as typed first-class fields — **out** unless Phase 1 freezes explicit mapping (path **A**) or stays blob-only. |

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

4. **`attributes` anti-scope:** **No** key under **`attributes`** may be treated as a G4 v1 **signal** without an explicit Phase 1 freeze on **path**, **type**, **redaction**, and **bounded meaning** (and tests). Until then, **`attributes`** remains an **opaque blob** in discovery terms.

5. **Explicitly not G4 v1 without new work:** Typed **Agent Card document**, **discovery URL**, **signature verification**, **issuer trust** — see [Explicitly out of G4 v1](#explicitly-out-of-g4-v1). **Promoting** arbitrary **`attributes`** keys to first-class card/discovery fields is **out** until a **Phase 1 freeze** names producer-stable paths.

6. **Spec vs adapter (`>=0.2 <1.0`)?** Capabilities advertise **`SUPPORTED_SPEC_VERSION_RANGE`** as `>=0.2 <1.0`. **Runtime validation** (`version.rs`) accepts **0.2+** (major `0`, minor ≥ `2`). That matches the P2b story: **shipped line is 0.x**, not a marketing claim of full **A2A v1.0** coverage. No code change required for this Phase 0 answer.

**Phase 0 gate (historical):** Discovery matrices + record above were **complete for the adapter codebase snapshot**. Evidence/Product review preceded **G4-A Phase 1**; implementation followed [Option A](#open-decision-phase-1-path-a-or-b) and is **merged on `main`** ([G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md)).

### Open decision — Phase 1 path (A or B)

The adapter **already** emits typed **`agent.capabilities`**. [Acceptance criteria](#acceptance-criteria-g4-done) ask for a discovery/card-related surface and “at least one **new** … signal.” Reviewers must **explicitly choose** one product path:

| | **Option A — new typed surface** | **Option B — bounded reuse (no new keys)** |
|---|----------------------------------|---------------------------------------------|
| **What ships** | **New** first-class field(s) or subobject in canonical emitted evidence (adapter mapping change). | **Hardening / clarification** on **existing** first-class `payload` — new **bounded meaning**, tests, examples; **no** new typed keys. |
| **Evidence-wave read** | **Preferred** for “G4 = evidence-wave” as a **new** observable seam. | **Fallback only** — acceptable **only if** G4 is **consciously narrowed** to a **hardening / bounded-semantics wave** on existing output, **not** co-equal with A for evidence-wave positioning. |
| **Phase 1** | Map agreed upstream or `attributes` paths; adapter + tests. | Docs + tests + examples only; adapter unchanged unless fixes are needed for unrelated reasons. |
| **Constraint** | — | **Option B must not** rename, market, or reframe existing typed payload as if a **new** discovery/card **seam** had been created. **B** is **bounded clarification** of observed meaning (docs / tests / examples), **not** a materially new evidence seam. |

**(A)** Phase 1 adds **new** first-class field(s) mapped from agreed upstream/`attributes` shapes. **(B)** G4 v1 treats **capability + identity visibility** as the **discovery-adjacent** wave and satisfies acceptance via **new bounded-meaning documentation, tests, and examples** on **existing** fields — **only if** product accepts **B** and applies the **§1 / §6** rows for **Option B** in [Acceptance criteria](#acceptance-criteria-g4-done).

**Reviewer / product note:** If G4 is positioned as a **materially new evidence-wave**, **A** is typically the stronger fit (the card/discovery-specific seam is **not** yet first-class in typed evidence). **B** is defensible for a **smaller** wave but must use **narrower outward wording** — **bounded semantics / clarification**, not “new seam.” Record **A** or **B** explicitly in the Phase 1 freeze so §1 / §6 are not argued twice in implementation.

**Recorded product preference:** **Option A preferred** — **G4** should introduce a **small, first-class** A2A discovery/card evidence seam; **visibility-first**, not validity-first. Bounded presence on **`agent.capabilities` alone** is **too thin** to carry the discovery/card wave; see **G4-A** below. **Option B** remains a **documented fallback** only when product **explicitly** chooses a **smaller** G4 (narrower outward framing — semantics / docs / tests on existing payload, **not** a new seam).

**Option B in one line:** Option **B** is acceptable **only if** G4 is **consciously narrowed** from a **new evidence-wave** to **bounded semantics and examples** on **existing typed payload** — **not** as a **co-equal** alternative to **A** for evidence-wave positioning, and **not** because “B is easier.”

## Phase 1 — Signal freeze

After Phase 0 is **reviewed and accepted**, Phase 1 freezes **either**:

- a small set of **2–4 typed fields**, **or**
- **one** small typed subobject,

**but not both** in G4 v1 unless discovery shows that the smaller shape alone would be **misleading**.

**Phase 0 codebase pass:** complete (see matrices + record). **Phase 1 field names** remain **frozen by review**, not by this document alone.

If Phase 1 follows **Option A**, use the **G4-A** proposal below as the working freeze (subject to path validation in [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/)). If Phase 1 follows **Option B** (**fallback**, consciously narrowed G4), use bounded semantics + tests + examples on **existing** `payload` fields only (see [Open decision — Phase 1 path (A or B)](#open-decision-phase-1-path-a-or-b)).

### Proposed Phase 1 freeze — G4-A (Option A, proposal)

**Goal:** Add **one** small typed **discovery/card** seam to **canonical emitted** A2A adapter evidence so G4 is a **materially new evidence-wave**, not only richer interpretation of **`agent.capabilities`**. **Second-order motivation** (spec ecosystem, security research around discovery/card surfaces) informs **why** this seam matters; **scope** stays **adapter-observable** and **bounded** per this PLAN.

#### Payload placement

G4-A proposes a **new top-level key** on the **emitted canonical A2A adapter `payload`** (the JSON object built in [`payload.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/payload.rs) today alongside `agent`, `task`, `artifact`, `message`, `attributes`, etc.) — e.g. `discovery` as a **sibling** of `agent`, **not** nested under `agent.*`, unless Phase 1 review explicitly records a different placement. This avoids architecture drift (“where did discovery live?”) at implementation time.

**Shape (conceptual):** one subobject under that key (exact key name frozen at implementation), e.g.:

```json
"discovery": {
  "agent_card_visible": true,
  "agent_card_source_kind": "attributes",
  "extended_card_access_visible": false,
  "signature_material_visible": false
}
```

Prefer **one subobject** over scattering top-level fields: one seam, clearer extension later, explicit “discovery/card” surface vs general agent metadata.

| Field | Type | Bounded meaning (may imply) | Must **not** imply |
|-------|------|-------------------------------|---------------------|
| `agent_card_visible` | bool | Observable discovery/card-related information is present **only** when **frozen** source rules fire (see threshold below) | Card valid, authentic, or complete |
| `agent_card_source_kind` | enum | Where visibility was derived from | Correctness of that source |
| `extended_card_access_visible` | bool | Observable evidence that an extended/authenticated card **surface** appeared in-band | Auth valid, client trusted, authorization sufficient |
| `signature_material_visible` | bool | **Only** the **presence** of material that matches **freeze-declared**, **bounded** signature-related paths (no parsing success, no crypto outcome) | Signature **valid**, signer **trusted**, provenance **verified**, **signing succeeded**, or **verification was attempted and passed** |

**Minimum threshold for `agent_card_visible` = true (normative for G4-A):**

Phase 1 must **harden** this into the formal freeze; until then the following is the **intent**:

- **`true` only if all** hold: **(1)** a match against a **pre-frozen set** of source paths or categories (typed payload, allowlisted `attributes`, or explicit unmapped rule — each **listed**, not guessed); **(2)** the matched source has a **minimum bounded shape** (typed columns or schema for that path — **not** a loose blob fragment); **(3)** the rule is **enumerated** in the freeze doc and tests, not inferred at runtime.
- **`true` is forbidden** from a **single** ad hoc `attributes` key, vague upstream hint, or heuristic (“something card-ish”) unless that key (or pattern) is **listed in the Phase 1 freeze** with required value shape.
- **Explicit non-trigger:** presence of **`agent.capabilities` alone** does **not** by itself set `agent_card_visible` **unless** the Phase 1 freeze adds a **named** rule (otherwise capabilities stay discovery-adjacent only, per Matrix A).

**Extra guardrail — `signature_material_visible`:** This field is the **fastest to sound like verification** and the **first candidate to drop** if repo-truth cannot support **freeze-declared** paths with honest tests. It may **only** mean: **bounded, pre-declared** signature-**related** material is **visible** at named paths — **not** “card signed,” **not** signature **valid**, **not** signer **trusted**, **not** provenance **established**. It must **never** encode signing **success**, verification **outcome**, or **provenance resolution** — only observable **presence** per freeze. Product copy must stay **visibility-only** (same discipline as freeze rule 3).

**Suggested enum (`agent_card_source_kind`):** `typed_payload` \| `attributes` \| `unmapped` \| `unknown` — makes “visible = true” honest when the signal comes from **`attributes`** or **lossiness**, not only from typed columns.

**Explicitly not in G4-A v1 (names illustrative):** `signature_verified`, `agent_card_trusted`, `issuer_trust`, `card_url_verified`, `handoff_verified`, **`authorization_context_visible`** as a **G3-style** seam for A2A — anything that reads as **validity** or **trustworthiness** rather than **visibility**.

**Freeze rules (must hold before ship):**

0. **`attributes` → G4 signal (normative)** — **No** path under **`attributes`** may feed **any** `discovery.*` field unless that **exact path**, **JSON type expectation**, **redaction rule**, and **bounded meaning** are **frozen** in the Phase 1 freeze doc (and covered by tests). No path, no signal.
1. **Source paths frozen** — Phase 1 implementation must list **exact** upstream keys / `attributes` paths (or “typed payload only” rules) that may set each field; precedence when multiple sources exist.
2. **`attributes` allowlist only** — No free-text inference from the blob; only **pre-frozen** key patterns; no “something card-like was in `attributes`” without a named path rule.
3. **Presence stays presence** — Product and docs may say e.g. “signature material **visible**”; never “signed card **verified**” from these fields alone.
4. **Adapter-first** — Implementation starts in [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/); [`assay-evidence`](../../crates/assay-evidence/) changes only if a bounded classification seam is truly required.
5. **Drop rules** — Phase 1 must document when a discovery/card **hint** in upstream input **must not** be promoted to typed `discovery.*` (guard against seam inflation). Hints that fail frozen path/shape rules stay **out of** the typed seam.

**Proposed Phase 1 acceptance (G4-A):**

1. A **new**, **small**, **visibility-first** first-class discovery/card **seam** appears in **emitted canonical** A2A evidence: **one** typed **`discovery`** (or equivalently named) **subobject** at **top-level** `payload` per [Payload placement](#payload-placement) — not a loose synonym for “any new signal.”
2. At least one **representative emitted JSON** example includes that **subobject** (docs or tests).
3. **Bounded meaning** per field is documented (may / must-not), aligned with the field table above.
4. Tests prove **`attributes`** keys are **not** promoted without **frozen** path rules (including **`agent_card_visible`** threshold).
5. **No** field in that seam implies **validity**, **trustworthiness**, **verification**, **signing success**, **provenance success**, or **verification outcome** — **observed visibility / presence only**, consistent with **visibility-first, not validity-first**.

### Phase 1 formal freeze — artifacts still required (reviewer checklist)

This PLAN is **complete enough** for **discovery review** and **direction** (Phase 0 + G4-A **proposal**). A **formal Phase 1 signal freeze** (ready for implementation PR) still requires **closing** the remaining **semantic** items below — they are **freeze deliverables**, not strategy gaps.

| # | Deliverable | Purpose |
|---|-------------|---------|
| 1 | **Per-field source mapping table** — for each `discovery.*` field, at minimum: **field name**, **source path(s)**, **minimum condition for `true` / non-default enum**, **may use `attributes`? (Y/N + which paths)**, **redaction / sensitivity** | Removes ambiguity on **when** each field may be set; implements **`agent_card_visible`** and **`signature_material_visible`** thresholds mechanically. |
| 2 | **Representative emitted JSON** — **grounded** in the **current** adapter payload layout (top-level siblings like `agent`, `attributes`, …), not only a conceptual snippet; includes **`discovery`** with at least one realistic **`agent_card_source_kind`** | Proves placement and shape **before** code argues structure. |
| 3 | **Explicit drop rules** — when upstream/`attributes` hints **must not** be promoted to typed `discovery.*` | Prevents inflation (“everything becomes visible”). Complements freeze rule 5 above. |

**Executable freeze (filled):** The **[G4-A Phase 1 formal freeze](G4-A-PHASE1-FREEZE.md)** document contains the **per-field mapping tables**, **precedence with examples**, **hard defaults**, **two full emitted-payload JSON examples**, **negative test matrix**, **`signature_material_visible` v1 decision** (deferred), and **assay-evidence** scope line — so **1b** can proceed adapter-first without reopening semantics.

**Remaining semantic closures (owned by Phase 1 freeze, not this PLAN PR):**

- **`agent_card_visible` = true** — must be **fully specified** via the mapping table: frozen paths **+** minimum bounded shape **+** exclusion of mere blob fragments (see **Minimum threshold for `agent_card_visible`** in the G4-A section above).
- **`signature_material_visible`** — ship **only** with **named** freeze-declared paths; if repo-truth is **too thin**, **drop or defer** this field **first** rather than weak guessing.
- **Payload placement** — **confirm** top-level `payload.discovery` (or frozen name) as the **canonical** location in the freeze doc so implementation does not reopen placement.

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

**How to read §1 and §6 depends on the Phase 1 path ([Open decision — Phase 1 path (A or B)](#open-decision-phase-1-path-a-or-b)):**

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

**P2c — A2A Discovery / Card Follow-Up Pack** productizes **lint/pack rules** *after* G4 evidence ships — visibility rules aligned to G4 `payload.discovery` signals. **Pack YAML lives in** [PLAN-P2c](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md) **and** the built-in / `packs/open/` artifacts on `main` (**`a2a-discovery-card-followup`**, rules A2A-DC-001 / A2A-DC-002). This G4 PLAN does not duplicate pack contents.

## Reviewer checks (suggested)

### Reviewer focus (Phase 0 → Phase 1)

The review question is **not** “is G4 a good idea?” but:

1. Is Phase 0 **sufficiently grounded and honest** (matrices + spec-vs-adapter + no premature code)?
2. Which **Phase 1 path** applies — **A** (preferred: new typed surface / G4-A) vs **B** (**fallback only**: consciously narrowed G4 — bounded reuse + docs/tests/examples on existing payload)?
3. Are [Acceptance criteria](#acceptance-criteria-g4-done) **correct for the chosen path** — especially if **B** (no new typed keys in G4 v1)?

### General checks

- PLAN-G4 does **not** promise **full A2A v1.0 coverage** beyond current **shipped adapter** reality (`SUPPORTED_SPEC_VERSION_RANGE` / version gate).
- Hypothesis buckets are labeled as such in reviews; ROADMAP/RFC sequencing does not treat them as frozen scope.

## References

- [PLAN-P2b — A2A Signal Follow-Up Claim Pack](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — P2b boundary; read the section **Adapter & protocol version reality (0.x)** in that document for the version gate.
- [RFC-005 — Trust compiler MVP](RFC-005-trust-compiler-mvp-2026q2.md) §6 sequencing.
- [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — SSOT for consumer/version floors if G4 implies contract or `requires` changes.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — consult before adding new pack check types in a follow-on **P2c** wave.
- [PLAN-P2c — A2A Discovery / Card Follow-Up Pack](PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md) — working plan for the P2c pack wave.
- [ROADMAP](../ROADMAP.md) — high-level sequencing (G4-A Phase 1 + **P2c** shipped on `main`); hypotheses stay in this PLAN.
