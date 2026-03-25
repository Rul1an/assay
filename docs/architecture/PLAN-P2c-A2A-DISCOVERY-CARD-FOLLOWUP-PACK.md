# PLAN — P2c A2A Discovery / Card Follow-Up Pack

- **Status:** **Skeleton / planning** — freeze sections below are the **design contract**; **no pack YAML, built-in registration, or parity tests** ship until Phase 1 freeze is accepted and a separate implementation PR follows this document.
- **Date:** 2026-03-25
- **Owner:** Evidence / Product
- **Prerequisite:** [G4-A Phase 1](G4-A-PHASE1-FREEZE.md) — `payload.discovery` on emitted canonical A2A evidence — **merged on `main`**.

## Sequencing discipline (why this order)

This wave follows the same discipline that made G4-A strong:

1. **Evidence seam** (adapter emits bounded shapes) — **done** (`payload.discovery`).
2. **Freeze** (normative semantics) — **done** ([G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md)).
3. **Pack productization** (this PLAN) — **plan and freeze first**, then YAML + tests in a **later** PR.

Do **not** commit pack YAML or rules in the same step as unfinished discovery rows in [§ Phase 0 — Pack engine discovery](#phase-0--pack-engine-discovery).

## Relation to G4-A `payload.discovery`

| G4-A (evidence) | P2c (pack) |
|-----------------|------------|
| Defines **what** appears on `/data/discovery/*` and **bounded meaning** (§2b, §4). | Defines **which lint rules** evaluate those fields in bundles — **no new semantics** and **no stronger claims** than the freeze. |
| Adapter-first, Assay-namespaced `attributes.assay_g4`. | Reads **canonical emitted** JSON only; does not invent upstream protocol types. |

Normative semantics for each field remain in **G4-A**; P2c `help_markdown` and pack **disclaimer** must **not** contradict §2b.

## Goal

Ship a **small, bounded** companion pack on **`assay.adapter.a2a.*`** evidence that can observe **[G4-A `payload.discovery`](G4-A-PHASE1-FREEZE.md)** signals — **parallel in spirit to [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)**, **not** an “A2A security”, trust, or verification pack.

## In scope (v1)

- Rules that are **evidence-only** and **visibility / presence** aligned to G4-A (see [§ Rule IDs — freeze target](#rule-ids--freeze-target-2-or-3-rules)).
- Composition with **P2b** (`a2a-signal-followup`): P2c **must not** duplicate P2b’s three `event_type_exists` rules unless product explicitly wants redundant checks.

## Out of scope

| Topic | Reason |
|-------|--------|
| Card **authenticity**, **signature validity**, **trusted provenance** | Not in G4-A v1 seam; pack must not imply them. |
| **Auth / authz correctness**, **G3**-style claims on A2A | No G3-shaped evidence on A2A adapter output. |
| Broad **“A2A security”** or protocol marketing claims | Same bar as [P2b non-goals](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md#explicitly-out-of-v1-non-rules). |
| **Pack engine** / `ENGINE_VERSION` **bump** | Only if Phase 0 proves existing check types are insufficient — default is **no bump**. |
| **New adapter or evidence signals** in the P2c wave | P2c is **pack-only** after G4-A; adapter changes belong to a **different** wave. |

## Pack identity — freeze target

| Field | Freeze value (proposal) |
|-------|-------------------------|
| **Pack `name` (YAML)** | `a2a-discovery-card-followup` |
| **Built-in id** | Same string as `name` (match P2b pattern) |
| **Version** | `1.0.0` at first ship (semver; bump only on rule/semver-worthy change) |
| **Kind** | `security` (same family as P2b) |
| **Rule count (v1)** | **2** preferred; **max 3** — see below |

## Rule IDs — freeze target (2 or 3 rules)

Prefix **`A2A-DC`** (**D**iscovery **C**ard follow-up) avoids collision with P2b **`A2A-001`..`003`**.

| ID | Intent (one line) | Bounded meaning (may imply) | Must **not** imply |
|----|-------------------|----------------------------|---------------------|
| **A2A-DC-001** | **Agent card visibility observed** — at least one canonical A2A event has **`agent_card_visible: true`** on `payload.discovery` (per G4-A paths). | Producer **asserted** the allowlisted visibility flag as `true` with valid upstream shape (see G4-A §4.1). | Card authentic, unspoofed, complete, or “real”. |
| **A2A-DC-002** | **Extended card access visibility observed** — at least one event has **`extended_card_access_visible: true`**. | Same discipline as §4.2 / §2b — **observed** flag only. | Auth **succeeded**, authz **held**, access **legitimate**, client **trusted**. |
| **A2A-DC-003** (optional) | **Source-kind visibility discipline** — e.g. require `agent_card_source_kind` **not** `unknown` when product wants “kind resolved”, or other **narrow** predicate. | Which **precedence class** is recorded (per §3). | Correctness or trustworthiness of that source. |

**Rule count advice:** Ship **DC-001 + DC-002** (two rules) unless product clearly needs **DC-003**. The third rule is easy to over-interpret; include only if it has **concrete** product value and stays **non-judgmental** (no “good/bad” source).

## Explicit non-rules (v1)

These are **not** P2c v1 rules (list may extend at freeze):

- Any rule that treats `discovery` as **verification** or **security outcome**.
- Any rule requiring **`signature_material_visible: true`** (v1 adapter always emits `false` per G4-A §1).
- Duplicates of **A2A-001..003** (P2b) unless explicitly desired.

## Phase 0 — Pack engine discovery

Before **any** YAML, confirm:

| # | Question | Done when |
|---|----------|-----------|
| 1 | Can **`json_path_exists`** (and optional **`event_types`** scoping) express DC-001 / DC-002, or do we need **boolean `true`** checks? | Documented; if value checks are required, validate **`conditional`** shape per [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) or record a **minimal** engine follow-up (separate PR). |
| 2 | Behavior for **pre-G4-A bundles** (no `/data/discovery`): fail, skip, or N/A? | Frozen in pack disclaimer + rule help. |
| 3 | **`requires.assay_min_version`** — floor that matches **first release** with `payload.discovery` on the adapter. | SSOT: [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) + release notes; same discipline as [PLAN-P2b § assay_min_version](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md#assay_min_version-pack-requires--release-truth). |
| 4 | **`evidence_schema_version`** | Align with other built-ins (e.g. `1.0`) unless migration doc says otherwise. |

## Acceptance — plan freeze vs implementation

| Gate | Criteria |
|------|----------|
| **Plan / freeze acceptable** | Pack identity table filled; **2–3** rule IDs with **bounded meaning** rows; Phase 0 table answered; non-rules listed. |
| **Implementation PR ready** | Built-in YAML + **`packs/open/`** mirror byte-for-byte parity (P2b pattern); **`assay-evidence`** tests for open/builtin equivalence; `requires` match release truth; docs (ROADMAP / this PLAN) synced. |

## Parity, release floor, and docs sync

- **Parity:** Same as P2b — built-in pack and `packs/open/a2a-discovery-card-followup/` mirror **must** match; tests must fail on drift.
- **Release floor:** `assay_min_version` must not claim packs against Assay versions that **cannot** emit `discovery` (document first tag in release notes when the pack ships).
- **Docs:** [ROADMAP](../ROADMAP.md) checklist row for P2c; optional one-line in [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md) sequencing if maintainers want cross-link.

## Implementation order (after freeze — not now)

1. Frozen **Phase 0** answers + final rule table.
2. **YAML** (built-in + open mirror).
3. **Tests** (parity + bundle fixtures with `payload.discovery`).
4. **Docs** sync and release note (first version embedding the built-in).

**Do not** start step 1 until this skeleton is reviewed and Phase 0 is complete.

## References

- [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md) — normative `payload.discovery` semantics.
- [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md) — [§ P2c](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md#p2c--follow-on-not-g4).
- [PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — precedent for companion packs.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — check types.
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — sequencing.

## Changelog

| Date | Change |
|------|--------|
| 2026-03-25 | Skeleton: file `PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md`; sequencing (evidence→freeze→pack); scope/non-goals; pack identity; **A2A-DC-001..003** with bounded-meaning rows; Phase 0; acceptance/parity/release floor; implementation order (YAML last). |
