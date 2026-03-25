# PLAN — P2c A2A Discovery / Card Follow-Up Claim Pack

- **Status:** **Planning** — scope and rule hypotheses below; **no pack YAML shipped** until Phase 1 freeze is explicitly accepted.
- **Date:** 2026-03-25
- **Owner:** Evidence / Product
- **Prerequisite:** [G4-A Phase 1](G4-A-PHASE1-FREEZE.md) (`payload.discovery` on emitted canonical A2A evidence) **merged on `main`**.

## Post-merge hygiene (record)

On `main` after G4-A + status-sync docs: `cargo test -p assay-adapter-a2a` and `assay-evidence` tests touching **`a2a-signal-followup`** parity were run successfully. This does not replace CI; it records local release-truth smoke.

## Goal

Ship a **small** companion pack (working name **`a2a-discovery-card-followup`**) that productizes **bounded lint rules** on the **G4-A `payload.discovery` seam** plus coherent use of existing **`assay.adapter.a2a.*`** evidence — **parallel in spirit to [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)**, **not** a trust, verification, or “A2A security” pack.

## North star

**P2c is a pack-wave**, not an evidence-wave. Evidence shapes are **frozen in the adapter** and [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md). P2c only **reads** those shapes from bundles.

## Relationship to other waves

| Wave | Role |
|------|------|
| **G4-A Phase 1** | Shipped **adapter** `payload.discovery` + freeze + tests. |
| **[P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)** | Presence-only on **`event_type`** (`A2A-001`..`003`) — no `discovery` fields. |
| **P2c (this plan)** | Rules that can reference **`/data/discovery/...`** on canonical A2A events (see [JSON Pointer](#json-pointer-and-event-envelope) below). |

**Composition:** Consumers may run **`a2a-signal-followup` + `a2a-discovery-card-followup`** together; P2c must **not** duplicate P2b’s three presence rules with different IDs unless product explicitly wants redundancy.

## Bounded meaning (normative for pack text)

Pack disclaimers and rule `help_markdown` MUST stay aligned with [G4-A-PHASE1-FREEZE.md §2b](G4-A-PHASE1-FREEZE.md#2b-bounded-meaning--may-imply--must-not-imply-all-four-fields):

- **`agent_card_visible` / `extended_card_access_visible`:** observed **visibility flags** on the frozen allowlisted paths — **not** authenticity, authorization success, or trusted client.
- **`agent_card_source_kind`:** which **class** of source won precedence — **not** strength or correctness of that source.
- **`signature_material_visible`:** in adapter v1 always **`false`** — pack rules must **not** imply verification.

## JSON Pointer and event envelope

Canonical evidence uses CloudEvents-style JSON; the adapter payload is under the **`data`** field on the event (`serde` name for `EvidenceEvent.payload`).

Examples (RFC 6901 pointers against the **full serialized event**):

| Target | Example pointer |
|--------|---------------------|
| Whole discovery object | `/data/discovery` |
| Agent card visibility | `/data/discovery/agent_card_visible` |
| Source kind string | `/data/discovery/agent_card_source_kind` |
| Extended access visibility | `/data/discovery/extended_card_access_visible` |

**Old bundles:** Events emitted **before** G4-A may lack `/data/discovery`. Pack rules must be explicit whether they **require** modern adapter output (fail closed) or **only apply when paths exist** — Phase 1 freeze must record this.

## Phase 0 — Discovery (before pack freeze)

| # | Question | Outcome when done |
|---|----------|-------------------|
| 1 | Which **check types** from [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) suffice for v1 (`json_path_exists`, `event_type_exists`, `conditional`, …)? | Written in Phase 1 table |
| 2 | Do we need **boolean value** checks (`true` only) or is **path existence** enough for v1? | If value checks are required, confirm **`conditional`** engine support or scope a minimal engine extension (separate decision; avoid scope creep). |
| 3 | **`assay_min_version` floor** — G4-A shipped in a given workspace release; pack `requires` must not claim bundles that predate adapter `discovery`. | SSOT: [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) + release notes for first tag containing `payload.discovery`. |
| 4 | **Built-in pack name + version** + parity with `packs/open/` mirror (same discipline as P2b). | Frozen in Phase 1 |

## Phase 1 — Pack freeze (proposal — not final)

| Field | Proposed value |
|-------|----------------|
| Pack name | `a2a-discovery-card-followup` (TBD — must not collide with `a2a-signal-followup`) |
| Kind | `security` (same family as P2b) |
| Rule IDs | **`A2A-D001`…** (prefix **`D`** = discovery seam; numeric range TBD at freeze) |

### Candidate rules (hypotheses — counts and IDs may change)

| ID | Intent | Likely check family | Notes |
|----|--------|----------------------|--------|
| **A2A-D001** | At least one `assay.adapter.a2a.*` event exposes a **`discovery` object** with expected keys (structural sanity). | `json_path_exists` on scoped `assay.adapter.a2a.*` | Confirms G4 seam **present** in bundle (for adapter versions that emit it). |
| **A2A-D002** | **Optional:** At least one event has **`agent_card_visible: true`** when product requires “card visibility signal observed.” | May need **value** predicate — **conditional** or engine follow-up | Do not ship until freeze accepts semantics and implementation path. |
| **A2A-D003** | **Optional:** Same pattern for **`extended_card_access_visible: true`**. | Same as D002 | Strict product review per [G4-A §4.2](G4-A-PHASE1-FREEZE.md#42-extended_card_access_visible). |
| **A2A-D004** | **Optional:** Discipline on **`agent_card_source_kind`** (e.g. forbid unexpected enum strings) — likely **not** v1 unless trivially expressible. | TBD | Prefer **defer** unless zero-risk. |

**v1 rule count target:** **small** (often **1–3** rules first ship); avoid duplicating P2b’s event-type presence story.

## Explicitly out of P2c v1

| Topic | Why |
|-------|-----|
| Agent Card **authenticity**, **signing**, **issuer trust** | Out of evidence seam; pack must not claim. |
| **Authorization / G3** parity on A2A | No G3-shaped fields on A2A evidence. |
| **“A2A security”** or broad protocol compliance | Same discipline as [P2b non-goals](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md#explicitly-out-of-v1-non-rules). |
| **Engine version bump** | Avoid unless a check type is genuinely missing; prefer existing **`json_path_exists`** / **`event_type_exists`** / narrow **`conditional`**. |

## Engine

- Default assumption: **Pack engine v1.2** (`ENGINE_VERSION` in [`checks.rs`](../../crates/assay-evidence/src/lint/packs/checks.rs)) — same as P2b unless this plan records a **required** extension.
- Any new `CheckDefinition` variant is a **separate** evidence-substrate change and must be justified in implementation PRs.

## Acceptance — plan vs implementation

| Gate | Definition |
|------|------------|
| **This PLAN acceptable** | Phase 0 table filled; Phase 1 freeze row finalized; disclaimers reviewed against §2b. |
| **Pack implementation ready** | Built-in YAML + `packs/open/` mirror + tests (parity pattern from P2b); `requires.assay_min_version` set from release truth. |

## References

- [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md) — normative semantics for `payload.discovery`.
- [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md) — G4 sequencing; [§ P2c](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md#p2c--follow-on-not-g4).
- [PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — companion pack precedent.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — check types and constraints.
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — trust compiler sequencing.

## Changelog

| Date | Change |
|------|--------|
| 2026-03-25 | Initial PLAN: discovery table, pointers, candidate rules, non-goals, hygiene record. |
