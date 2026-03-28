# PLAN — P2c A2A Discovery / Card Follow-Up Pack

- **Status:** **v1 shipped on `main`** (2026-03-25) and now publicly released in **`v3.4.0`** — built-in pack **`a2a-discovery-card-followup`**, open mirror, parity tests, and minimal **`json_path_exists.value_equals`** engine support merged ([§ Implementation order](#implementation-order-implementation-pr)). Phase 0 decisions below remain the **contract** for this pack; future rule changes require PLAN updates.
- **Date:** 2026-03-25
- **Owner:** Evidence / Product
- **Prerequisite:** [G4-A Phase 1](G4-A-PHASE1-FREEZE.md) — `payload.discovery` on emitted canonical A2A evidence — **merged on `main`**.

## Sequencing discipline (why this order)

This wave follows the same discipline that made G4-A strong:

1. **Evidence seam** (adapter emits bounded shapes) — **done** (`payload.discovery`).
2. **Freeze** (normative semantics) — **done** ([G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md)).
3. **Pack productization** (this PLAN) — **plan and freeze first**, then YAML + tests in a **later** PR.

Phase 0 is **complete** — see [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked). Do **not** commit pack YAML or rules until the **implementation PR** follows the sequencing in [§ Implementation order](#implementation-order-implementation-pr).

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

- Rules that are **evidence-only** and **visibility / presence** aligned to G4-A (see [§ Rule IDs — freeze target](#rule-ids-freeze-target-2-rules)).
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

| Field | Freeze value |
|-------|-------------------------|
| **Pack `name` (YAML)** | `a2a-discovery-card-followup` |
| **Built-in id** | Same string as `name` (match P2b pattern) |
| **Version** | `1.0.0` at first ship (semver; bump only on rule/semver-worthy change) |
| **Kind** | `security` (same family as P2b). Product confirms this categorization remains appropriate for **observed visibility-only** rules (no alternate taxonomy debate in this plan). At implementation, YAML / `help_markdown` / disclaimer must **not** imply generic “A2A security” outcomes **solely** because `kind` is `security` — keep outward copy as narrow as the rules. |
| **Rule count (v1)** | **2** — [A2A-DC-001](#rule-ids-freeze-target-2-rules) and [A2A-DC-002](#rule-ids-freeze-target-2-rules) only; [DC-003](#deferred-opt-in-only-a2a-dc-003) is **not** v1. |

## Rule IDs — freeze target (2 rules)

Prefix **`A2A-DC`** (**D**iscovery **C**ard follow-up) avoids collision with P2b **`A2A-001`..`003`**.

| ID | Intent (one line) | Bounded meaning (may imply) | Must **not** imply |
|----|-------------------|----------------------------|---------------------|
| **A2A-DC-001** | **Agent card visibility observed** — at least one canonical A2A event has **`agent_card_visible: true`** on `payload.discovery` (per [G4-A](G4-A-PHASE1-FREEZE.md) paths). | Producer **asserted** the allowlisted visibility flag as `true` with valid upstream shape (see G4-A §4.1). | Card authentic, unspoofed, complete, or “real”. |
| **A2A-DC-002** | **Extended card access visibility observed** — at least one canonical A2A event has **`extended_card_access_visible: true`** on `payload.discovery` (per [G4-A](G4-A-PHASE1-FREEZE.md) paths). | Producer **asserted** the allowlisted extended-card-access visibility flag as `true` with valid upstream shape (see G4-A §4.2 / §2b). | Auth **succeeded**, authz **held**, access **legitimate**, client **trusted**. |

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

Phase 0 was a **discovery** pass on pack-engine fit, CI semantics, and release floors. Decisions are **locked** in [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked). The table below records the original questions; the **Done when** column reflects the locked outcome.

| # | Question | Done when |
|---|----------|-----------|
| 1 | Can pack checks express **boolean JSON `true`** on **G4-A-frozen** `/data/discovery/...` paths for DC-001 / DC-002 — i.e. **value** `true`, not merely “path/key exists”? **SPEC today:** `json_path_exists` is **presence-only** (a `false` value still satisfies “path exists”), and `conditional` is an if/then **required-path** pattern — it does **not** mean “at least one event satisfies `== true`” on its own. **Locked intent:** implementation PR adds a **minimal** engine affordance for value equality at a frozen pointer (e.g. optional `value_equals` on `json_path_exists`) or an equivalent narrow check; **no** broad `ENGINE_VERSION` bump unless unavoidable. | **Locked:** Prefer **smallest** change that makes **`== true`** honest; **no gratuitous engine bump** (see **decision 2** under [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked)). |
| 2 | **Pre-G4-A bundles** (no `/data/discovery`): default **fail** in normal lint for rules that require discovery, **consistent** with chosen check types — unless the team **explicitly** chooses skip/N/A in Phase 0 and documents **why** (CI consistency). Disclaimer + `help_markdown` must state the pack requires **G4-A discovery emission**. **Default is for CI clarity** and must read consistently across **rule descriptions**, **help text**, and **example outputs** (avoid plan-says-fail vs help-reads-N/A). This product/CI default means a rule **reports missing required discovery evidence for this pack**, **not** that the bundle is generally malformed or invalid. | **Locked:** **Fail by default** for DC-001 / DC-002; disclaimer + help explicit (see **decision 3** under [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked)). |
| 3 | **`requires.assay_min_version`** — **do not** assume the same floor as [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) (e.g. `>=3.2.3`) by default. **Anchor** to the **first Assay release** that ships **G4-A `payload.discovery`** in release binaries/artifacts. The **exact semver string stays intentionally unset in this plan until pack ship**; set in YAML + release notes at ship time. Meaning of `requires` floors: [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md). | **Locked:** String **unset in this plan until ship**; floor = first G4-A-capable release, **not** P2b’s floor (see **decision 4** under [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked)). |
| 4 | **`evidence_schema_version`** | **Locked:** **`1.0`**, aligned with other built-ins, unless [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) or SSOT gives a reason to change (see **decision 5** under [§ Phase 0 — decisions (locked)](#phase-0-decisions-locked)). |

### Phase 0 — decisions (locked)

**Locked:** 2026-03-25. These decisions are **product/engine contract** for the P2c implementation PR; they do **not** change G4-A normative semantics.

**One-line summary:** Ship a **minimal two-rule** P2c on the G4-A discovery seam (**DC-001**, **DC-002**); **minimal engine delta** so **`== true`** is honest (see decision 2); **fail** pre-G4-A bundles by default for CI clarity; keep **`assay_min_version` unset** in this plan until the first **G4-A-capable** Assay release at pack ship; **`evidence_schema_version` = 1.0**; **defer DC-003** and any **signature/provenance** semantics beyond G4-A.

1. **v1 stays exactly two rules.** Ship only **A2A-DC-001** (`agent_card_visible == true`) and **A2A-DC-002** (`extended_card_access_visible == true`). **A2A-DC-003** remains [deferred / opt-in](#deferred-opt-in-only-a2a-dc-003). **Rationale:** A2A positions Agent Cards and extended card retrieval as core discovery surfaces; recent agent-security work (e.g. **A2ASecBench**-style emphasis on discovery/card spoofing as a risk surface) supports **visibility-on-the-seam** first, not an extra “source-kind discipline” rule that drifts toward judgment semantics in v1.

2. **No gratuitous engine bump; smallest change for honest `== true`.** Raw **`json_path_exists`** (SPEC) does **not** distinguish `true` vs `false` — only path presence. **Locked:** the P2c **implementation PR** includes a **minimal** pack-engine extension (or equivalent) so rules evaluate **JSON boolean `true`** at the frozen `/data/discovery/...` pointers, without misleading “key present” semantics. **Rationale:** reuse narrow primitives; avoid a new policy DSL — but **do not** pretend the pre-P2c engine already enforced value equality.

3. **Pre-G4-A bundles: fail by default in normal lint.** Bundles without `/data/discovery` cause **DC-001 / DC-002** to **fail** (consistent with chosen checks). **Disclaimer** and **`help_markdown`** must state the pack **requires G4-A discovery emission**. Skip/N/A only if explicitly chosen and **documented with rationale**. **Rationale:** clearest CI signal (“this pack needs newer evidence”), not a general claim that the bundle is invalid.

4. **`assay_min_version`:** **no** exact version string in this plan. At ship, set the floor to the **first Assay release** that includes **G4-A `payload.discovery`** in release binaries/artifacts — **not** [P2b](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md)’s substrate floor. **Rationale:** P2c depends on the **G4-A seam**, so the floor must track that seam.

5. **`evidence_schema_version`:** **`1.0`**, aligned with other built-ins, unless migration / SSOT gives an explicit reason to differ.

**Non-normative context (March 2026):** Industry and research emphasis on **identity, authorization, auditing, and observed evidence** for agents aligns with **small, observable** pack rules — not broad “secure agent” marketing claims. This plan stays within [§ Explicit non-rules](#explicit-non-rules-v1) and **does not** reopen G4-A’s deferral on **`signature_material_visible`**.

## Acceptance — plan freeze vs implementation

| Gate | Criteria |
|------|----------|
| **Plan / freeze acceptable** | Pack identity table filled; **2** rule IDs (**DC-001**, **DC-002**) with **bounded meaning** rows; [Deferred DC-003](#deferred-opt-in-only-a2a-dc-003) documented as **not** v1; [Phase 0 decisions](#phase-0-decisions-locked) **locked**; non-rules listed. |
| **Implementation PR ready** | Built-in YAML + **`packs/open/`** mirror byte-for-byte parity (P2b pattern); **`assay-evidence`** tests for open/builtin equivalence; `requires` match release truth; docs (ROADMAP / this PLAN) synced. |

## Parity, release floor, and docs sync

- **Parity:** Same as P2b — built-in pack and `packs/open/a2a-discovery-card-followup/` mirror **must** match; tests must fail on drift.
- **Release floor / `requires`:** [MIGRATION-TRUST-COMPILER-3.2.md — P2c pack section](MIGRATION-TRUST-COMPILER-3.2.md#a2a-discovery-card-followup-built-in-pack-p2c) only (substrate vs G4-A/P2c floors, `value_equals`, no `ENGINE_VERSION` bump, first tag/binary); built-in + open YAML stay byte-identical.
- **Docs:** [ROADMAP](../ROADMAP.md) checklist, [CHANGELOG](../../CHANGELOG.md) [Unreleased], [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md) §6, [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) P2c section — synced for v1 ship.

## Implementation order (implementation PR)

Phase 0 is **locked** ([§ Phase 0 — decisions (locked)](#phase-0-decisions-locked)). **Steps 1–4** below are **complete** for P2c v1 on `main` (merge 2026-03-25).

1. ~~Frozen **Phase 0** answers + final rule table~~ — **done** (this PLAN on `main`).
2. ~~**YAML** (built-in + open mirror)~~ — **done** (`crates/assay-evidence/packs/a2a-discovery-card-followup.yaml`, `packs/open/a2a-discovery-card-followup/`).
3. ~~**Tests** (parity + bundle fixtures with `payload.discovery`)~~ — **done** (`crates/assay-evidence/tests/a2a_discovery_card_followup_pack.rs` and related).
4. ~~**Docs** sync and release note~~ — **done** (this PLAN + [CHANGELOG](../../CHANGELOG.md) [Unreleased]; ROADMAP checklist).

**Further changes** to rules or semantics require updating this PLAN (no ad-hoc pack edits without plan alignment).

## References

- [G4-A-PHASE1-FREEZE.md](G4-A-PHASE1-FREEZE.md) — normative `payload.discovery` semantics.
- [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md) — [§ P2c](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md#p2c-follow-on-not-g4).
- [PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md](PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md) — precedent for companion packs.
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — check types.
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md) — sequencing.

## Changelog

| Date | Change |
|------|--------|
| 2026-03-25 | Skeleton: file `PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md`; sequencing (evidence→freeze→pack); scope/non-goals; pack identity; **A2A-DC-001..003** with bounded-meaning rows; Phase 0; acceptance/parity/release floor; implementation order. |
| 2026-03-23 | Review pass: v1 = **2** rules in main table (**DC-001**, **DC-002**); **DC-003** moved to **Deferred / opt-in**; Phase 0 — boolean `true` on frozen paths, pre-G4-A fail default + CI copy consistency, `assay_min_version` anchored to G4-A seam (unset until ship); **Relation to P2b**; `kind: security` one-line product note; implementation order clarified (no YAML before freeze/Phase 0; remove “YAML last” ambiguity). |
| 2026-03-23 | Polish: implementation guard — **step 2** (YAML) blocked until review + Phase 0; Phase 0 row 2 — fail = **missing pack-required discovery**, not general bundle invalidity; **DC-002** bounded meaning parallel to **DC-001**; **`kind: security`** outward-copy caution; explicit non-rule — P2c v1 does **not** reopen G4-A **`signature_material_visible`** deferral. |
| 2026-03-25 | **Phase 0 locked** (+ **Copilot #947 follow-up**): v1 = **DC-001** + **DC-002** only; **DC-003** deferred; **fail** pre-G4-A bundles by default; **`assay_min_version` unset** until ship; **`evidence_schema_version` = 1.0**; status → Phase 0 locked; implementation order → step 2 next. Clarify row 1 / decision 2: SPEC **`json_path_exists`** is presence-only; P2c implementation PR needs **minimal** value-equality (or equivalent) for honest **`== true`** — not “existing checks already do this.” |
| 2026-03-25 | Implementation sync: pack ships **`requires.assay_min_version: ">=3.3.0"`** (built-in + open); SPEC **`value_equals`** — JSON equality only (no coercion); parity section updated. |
| 2026-03-25 | **Shipped on `main`:** P2c v1 implementation merged — built-in **`a2a-discovery-card-followup`** (A2A-DC-001 / A2A-DC-002), **`json_path_exists`** optional **`value_equals`**, ROADMAP / CHANGELOG / this PLAN post-merge sync. |
