# PLAN — P2b A2A Signal Follow-Up Claim Pack

- Status: **Implementation** (built-in pack `a2a-signal-followup`)
- Date: 2026-03-23
- Owner: Evidence / Product

## Goal

Ship a **small A2A-native companion pack** (parallel in spirit to P2a, **not** an MCP clone): **presence-only** rules on canonical **`assay.adapter.a2a.*`** evidence as **actually emitted** by [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/), without claiming authorization validity, card signing, or G3 parity unless those shapes exist first-class in bundles.

## Phase 0 — Discovery (repo-truth, satisfied before ship)

Discovery is recorded in the working plan annex and adapter sources; summary:

- **Primary truth** is **shipped adapter mapping** (`mapping.rs`, `payload.rs`), not the external A2A spec alone.
- **Canonical types** that are first-class today include `agent.capabilities` → `assay.adapter.a2a.agent.capabilities`, `task.requested` / `task.updated` → `assay.adapter.a2a.task.requested` / `…task.updated`, `artifact.shared` → `assay.adapter.a2a.artifact.shared`.
- **Not** first-class in typed payload: `assay.tool.decision`, G3 fields, signed Agent Card bytes — so **no** G3-reuse rule and **no** “auth validity” rule for P2b v1.
- **`attributes`** may carry opaque upstream data; P2b v1 does **not** promote free-form `attributes` to pack rules.
- **Delegation / handoff:** no smaller typed canonical field than generic task metadata was identified for a bounded handoff rule — **no** A2A handoff rule in v1 (and not inferred from `task.kind` alone).
- **Containment degradation** (`assay.sandbox.degraded`) is protocol-agnostic; it does **not** add A2A-specific value — **not** in this pack (same rationale as discovery: avoid MCP-003 repetition without A2A signal).

Extended discovery notes may live in a **local-only** Cursor plan under `.cursor/plans/` (not versioned here). **Shipped adapter truth** for mapping and payloads: [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) (`mapping.rs`, `payload.rs`, `convert.rs`, `version.rs`).

## Phase 1 — Pack freeze (v1)

| Field | Value |
|-------|--------|
| Name | `a2a-signal-followup` |
| Version | `1.0.0` |
| Rule count | **3** (A2A-001..003) |
| Check types | `event_type_exists` only |

## Why these three rules (and nothing else)

| Rule | Why it is in v1 |
|------|------------------|
| **A2A-001** | Adapter emits a dedicated canonical type for **capability discovery** (`agent.capabilities` → `assay.adapter.a2a.agent.capabilities`). The rule only asserts **that event type appears** — i.e. capability-discovery evidence is **present**, not that capabilities are correct, complete, or unspoofed. |
| **A2A-002** | Task lifecycle is visible through **`task.requested` and/or `task.updated`** mapped to canonical `assay.adapter.a2a.task.*` types. One globbed `event_type_exists` covers **both** emitted suffixes. |
| **A2A-003** | Artifact exchange is visible through **`artifact.shared`** → `assay.adapter.a2a.artifact.shared`. Wording stays **exchange visibility / observed shared artifact evidence** — not integrity, provenance, or “safe sharing.” |

## Explicitly out of v1 (non-rules)

| Topic | Why excluded |
|-------|----------------|
| **Authorization / G3-like** | No `assay.tool.decision` or G3-shaped fields on A2A bundles; reusing MCP-001 would be **theater**. |
| **Signed Agent Card / provenance** | Not first-class in adapter payload; could only appear untyped under `attributes` — not a v1 pack contract. |
| **Containment degradation** | Protocol-agnostic signal; no A2A-specific claim — intentionally omitted (same discipline as discovery). |
| **Delegation / handoff** | No bounded typed field for handoff; **`task.kind` alone is insufficient** per discovery — no rule. |
| **Engine bump** | All rules express with existing **`event_type_exists`**; **no** new `CheckDefinition` — **no** `ENGINE_VERSION` increase (avoids engine creep). |

## Engine

- **Pack engine v1.2** (unchanged for P2b): rules use existing **`event_type_exists`** — **no** new `CheckDefinition` and **no** `ENGINE_VERSION` bump.

**A2A-002 implementation note:** `event_type_exists.pattern` uses **glob** matching (same machinery as other pack rules). The pattern `assay.adapter.a2a.task.*` matches both **`assay.adapter.a2a.task.requested`** and **`assay.adapter.a2a.task.updated`**. Integration tests exercise **each** canonical type separately.

## Rule semantics (frozen)

| Rule | Check | Canonical signal |
|------|--------|-------------------|
| A2A-001 | `event_type_exists` | `assay.adapter.a2a.agent.capabilities` |
| A2A-002 | `event_type_exists` | `assay.adapter.a2a.task.*` (requested / updated) |
| A2A-003 | `event_type_exists` | `assay.adapter.a2a.artifact.shared` |

**Option A / B:** all **B** — presence-only; **no** kernel-sharing with MCP-001 / G3.

## `assay_min_version` (pack `requires`) — release truth

**SSOT:** [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — do not duplicate semantics elsewhere.

**YAML value (authoritative):** `requires.assay_min_version: ">=3.2.3"`.

**Meaning (same discipline as [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)):** `>=3.2.3` tracks the **evidence substrate** line (G3 + Trust Card schema 2 + seven claims; **v3.2.3** is the reference tag for that prerequisite). It is **not** automatically the “first tag that contains the built-in `a2a-signal-followup` binary.”

**At release:** state in release notes the **first published Assay version** that embeds **`a2a-signal-followup`**. Optionally bump workspace semver and tighten `assay_min_version` if you want the field to double as a pack floor — or keep `>=3.2.3` and document binary availability separately (see PLAN-P2a options).

## Adapter & protocol version reality (0.x)

The adapter ships **`SUPPORTED_SPEC_VERSION_RANGE`** consistent with **`>=0.2 <1.0`** and rejects upstream **`protocol_version`** with **major ≠ 0** at validation. **P2b v1** describes **Assay A2A adapter evidence as implemented today** (0.2.x-style upstream), **not** “full A2A v1.0 protocol coverage” or marketing labels the upstream spec may use.

## Non-goals (short)

Authorization validity, issuer trust, card trustworthiness, handoff validation, artifact integrity/safety, sandbox correctness, broad “A2A security.”

## References

- [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md)
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md)
