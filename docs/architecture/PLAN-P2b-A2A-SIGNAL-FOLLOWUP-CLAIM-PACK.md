# PLAN — P2b A2A Signal Follow-Up Claim Pack

- Status: **Implementation** (built-in pack `a2a-signal-followup`)
- Date: 2026-03-23
- Owner: Evidence / Product

## Goal

Ship a **small A2A-native companion pack** (parallel in spirit to P2a, **not** an MCP clone): **presence-only** rules on canonical **`assay.adapter.a2a.*`** evidence as **actually emitted** by [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/), without claiming authorization validity, card signing, or G3 parity unless those shapes exist first-class in bundles.

## Pack identity

| Field | Value |
|-------|--------|
| Name | `a2a-signal-followup` |
| Version | `1.0.0` |
| Rules | A2A-001, A2A-002, A2A-003 |

## Engine

- **Pack engine v1.2** (unchanged for P2b): rules use existing `event_type_exists` — **no** new `CheckDefinition` and **no** `ENGINE_VERSION` bump for P2b.

## Rule semantics (frozen)

| Rule | Check | Canonical signal |
|------|--------|------------------|
| A2A-001 | `event_type_exists` | `assay.adapter.a2a.agent.capabilities` |
| A2A-002 | `event_type_exists` | `assay.adapter.a2a.task.*` (requested / updated) |
| A2A-003 | `event_type_exists` | `assay.adapter.a2a.artifact.shared` |

**Optie A / B:** all **B** — presence-only; **no** kernel-sharing with MCP-001 / G3 unless future discovery adds typed A2A decision evidence (not in v1).

## `assay_min_version` (pack `requires`)

**SSOT:** [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md). Pack uses `>=3.2.3` like P2a — same evidence-substrate floor; confirm the **first published Assay version** that embeds `a2a-signal-followup` in release notes.

## Version caveat (adapter vs “A2A v1.0”)

The adapter documents `SUPPORTED_SPEC_VERSION_RANGE` and rejects upstream `protocol_version` with **major != 0** at validation time. P2b v1 describes **current shipped adapter bundles** (0.2.x-style), not full external-spec coverage.

## Non-goals

- G3-equivalent authorization rules on A2A, signed Agent Card / provenance rules, containment degradation, delegation/handoff rules from `task.kind` alone, authorization validity or issuer trust.

## Discovery reference

Detailed adapter inventory, payload paths, and matrix: [.cursor/plans/p2b_a2a_claim_pack_a7601fe9.plan.md](../../.cursor/plans/p2b_a2a_claim_pack_a7601fe9.plan.md) (Annex — repo-truth).

## References

- [PLAN-P2a](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)
- [RFC-005 §6](RFC-005-trust-compiler-mvp-2026q2.md)
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md)
