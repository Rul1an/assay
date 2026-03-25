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

Do **not** commit pack YAML or rules until [§ Phase 0 — Pack engine discovery](#phase-0--pack-engine-discovery) is complete and this document’s freeze rows are accepted.

## Relation to G4-A `payload.discovery`

| G4-A (evidence) | P2c (pack) |
|-----------------|------------|
| Defines **what** appears on `/data/discovery/*` and **bounded meaning** (§2b, §4). | Defines **which lint rules** evaluate those fields in bundles — **no new semantics** and **no stronger claims** than the freeze. |
| Adapter-first, Assay-namespaced `attributes.assay_g4`. | Reads **canonical emitted** JSON only; does not invent upstream protocol types. |

Normative semantics for each field remain in **G4-A**; P2c `help_markdown` and pack **disclaimer** must **not** contradict §2b.

## Relation to P2b (companion pack)

- **[P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)** asserts **canonical A2A event-type presence** (`event_type_exists` on `assay.adapter.a2a.*`).
- **P2c** reads **G4-A `payload.discovery`** on those same events.
- The packs may be **used together**; they are **semantically distinct** (different product surfaces — not duplicate claims on the same signal).

## Goal

Ship a **small, bounded** companion pack on **`assay.adapter.a2a.*`** evidence that can observe **[G4-A `payload.discovery`](G4-A-PHASE1-FREEZE.md)** signals — **parallel in spirit to [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)**, **not** an “A2A security”, trust, or verification pack.

## In scope (v1)

- Rules that are **evidence-only** and **visibility / presence** aligned to G4-A (see [§ Rule IDs — freeze target](#rule-ids--freeze-target-2-rules)).
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
| **Kind** | `security` (same family as P2b). Product confirms this categorization remains appropriate for **observed visibility-only** rules (no alternate taxonomy debate in this plan). At implementation, YAML / `help_markdown` / disclaimer must **not** imply generic “A2A security” outcomes **solely** because `kind` is `security` — keep outward copy as narrow as the rules. |
| **Rule count (v1)** | **2** — [A2A-DC-001](#rule-ids--freeze-target-2-rules) and [A2A-DC-002](#rule-ids--freeze-target-2-rules) only; [DC-003](#deferred--opt-in-only-a2a-dc-003) is **not** v1. |

## Rule IDs — freeze target (2 rules)

Prefix **`A2A-DC`** (**D**iscovery **C**ard follow-up) avoids collision with P2b **`A2A-001`..`003`**.

| ID | Intent (one line) | Bounded meaning (may imply) | Must **not** imply |
|----|-------------------|----------------------------|---------------------|
| **A2A-DC-001** | **Agent card visibility observed** — at least one canonical A2A event has **`agent_card_visible: true`** on `payload.discovery` (per [G4-A](G4-A-PHASE1-FREEZE.md) paths). | Producer **asserted** the allowlisted visibility flag as `true` with valid upstream shape (see G4-A §4.1). | Card authentic, unspoofed, complete, or “real”. |
| **A2A-DC-002** | **Extended card access visibility observed** — at least one event has **`extended_card_access_visible: true`**. | Same discipline as §4.2 / §2b — **observed** flag only. | Auth **succeeded**, authz **held**, access **legitimate**, client **trusted**. |

### Deferred / opt-in only: A2A-DC-003

**Not** part of P2c v1 freeze or default ship. Consider only if product explicitly opts in **and** this document is updated.

| Aspect | Content |
|--------|---------|
| **Intent (sketch)** | **Source-kind visibility discipline** — e.g. require `agent_card_source_kind` **not** `unknown` when product wants “kind resolved”, or another **narrow** predicate. |
| **Bounded meaning** | Which **precedence class** is recorded (per G4-A §3). |
| **Must not imply** | Correctness or trustworthiness of that source. |

**Opt-in gates (all required):**

1. Explicit **product justification** (user-visible value for “source-kind resolved” or equivalent).
2. **Phase 0** shows the rule is achievable **without** judgment / quality semantics.
3. Wording stays **strictly non-judgmental** (no implicit “good” vs “bad” source).

## Explicit non-rules (v1)

These are **not** P2c v1 rules (list may extend at freeze):

- Any rule that treats `discovery` as **verification** or **security outcome**.
- Any rule requiring **`signature_material_visible: true`** (v1 adapter always emits `false` per G4-A §1). P2c v1 **does not reopen** G4-A’s deferral on **`signature_material_visible`** semantics — Phase 0 must **not** treat signature visibility as an open design question.
- Duplicates of **A2A-001..003** (P2b) unless explicitly desired.

## Phase 0 — Pack engine discovery

Before **any** pack YAML (built-in or open mirror), confirm:

| # | Question | Done when |
|---|----------|-----------|
| 1 | Can **existing** pack checks express **boolean `true`** on **G4-A-frozen** `/data/discovery/...` paths for DC-001 / DC-002 — i.e. **value** `true`, not merely “path/key exists” where semantics require `true`? Prefer `json_path_exists`, `event_type_exists` + **`event_types`** scoping, and **`conditional`** per [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md). Engine follow-up only if **minimal** and boolean matching cannot be expressed without **misleading** pack semantics. | Documented; optional **minimal** engine PR if truly required. |
| 2 | **Pre-G4-A bundles** (no `/data/discovery`): default **fail** in normal lint for rules that require discovery, **consistent** with chosen check types — unless the team **explicitly** chooses skip/N/A in Phase 0 and documents **why** (CI consistency). Disclaimer + `help_markdown` must state the pack requires **G4-A discovery emission**. **Default is for CI clarity** and must read consistently across **rule descriptions**, **help text**, and **example outputs** (avoid plan-says-fail vs help-reads-N/A). This product/CI default means a rule **reports missing required discovery evidence for this pack**, **not** that the bundle is generally malformed or invalid. | Frozen in pack disclaimer + rule help. |
| 3 | **`requires.assay_min_version`** — **do not** assume the same floor as [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) (e.g. `>=3.2.3`) by default. **Anchor** to the **first Assay release** that ships **G4-A `payload.discovery`** in release binaries/artifacts. The **exact semver string stays intentionally unset in this plan until pack ship**; set in YAML + release notes at ship time. Meaning of `requires` floors: [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md). | Documented; concrete string at ship. |
| 4 | **`evidence_schema_version`** | Align with other built-ins (e.g. `1.0`) unless migration doc says otherwise. |

## Acceptance — plan freeze vs implementation

| Gate | Criteria |
|------|----------|
| **Plan / freeze acceptable** | Pack identity table filled; **2** rule IDs (**DC-001**, **DC-002**) with **bounded meaning** rows; [Deferred DC-003](#deferred--opt-in-only-a2a-dc-003) documented as **not** v1; Phase 0 table answered; non-rules listed. |
| **Implementation PR ready** | Built-in YAML + **`packs/open/`** mirror byte-for-byte parity (P2b pattern); **`assay-evidence`** tests for open/builtin equivalence; `requires` match release truth; docs (ROADMAP / this PLAN) synced. |

## Parity, release floor, and docs sync

- **Parity:** Same as P2b — built-in pack and `packs/open/a2a-discovery-card-followup/` mirror **must** match; tests must fail on drift.
- **Release floor:** `assay_min_version` must not claim packs against Assay versions that **cannot** emit `payload.discovery`. Until pack ship, treat the **exact floor string** as **unset in this plan**; tie it to the **first G4-A-capable release** when the pack ships (release notes / tag).
- **Docs:** [ROADMAP](../ROADMAP.md) checklist row for P2c; optional one-line in [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md) sequencing if maintainers want cross-link.

## Implementation order (after freeze — not now)

Do **not** author YAML until **freeze acceptance** and **Phase 0** rows above are complete.

1. Frozen **Phase 0** answers + final rule table (2 rules; DC-003 remains deferred unless opted in).
2. **YAML** (built-in + open mirror).
3. **Tests** (parity + bundle fixtures with `payload.discovery`).
4. **Docs** sync and release note (first version embedding the built-in).

**Do not** start **step 2** (YAML) until this skeleton is reviewed, Phase 0 is complete, and **step 1** (frozen Phase 0 answers + final rule table) is accepted.

## References

- [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md) — normative `payload.discovery` semantics.
- [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md) — [§ P2c](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md#p2c--follow-on-not-g4).
- [PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — precedent for companion packs.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — check types.
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — sequencing.

## Changelog

| Date | Change |
|------|--------|
| 2026-03-25 | Skeleton: file `PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md`; sequencing (evidence→freeze→pack); scope/non-goals; pack identity; **A2A-DC-001..003** with bounded-meaning rows; Phase 0; acceptance/parity/release floor; implementation order. |
| 2026-03-23 | Review pass: v1 = **2** rules in main table (**DC-001**, **DC-002**); **DC-003** moved to **Deferred / opt-in**; Phase 0 — boolean `true` on frozen paths, pre-G4-A fail default + CI copy consistency, `assay_min_version` anchored to G4-A seam (unset until ship); **Relation to P2b**; `kind: security` one-line product note; implementation order clarified (no YAML before freeze/Phase 0; remove “YAML last” ambiguity). |
| 2026-03-23 | Polish: implementation guard — **step 2** (YAML) blocked until review + Phase 0; Phase 0 row 2 — fail = **missing pack-required discovery**, not general bundle invalidity; **DC-002** bounded meaning parallel to **DC-001**; **`kind: security`** outward-copy caution; explicit non-rule — P2c v1 does **not** reopen G4-A **`signature_material_visible`** deferral. |
