# PLAN — H1 Trust Kernel Alignment & Release Hardening

- Status: **Shipped** on `main` (docs + alignment tests; no new product semantics)
- Date: 2026-03-24
- Owner: Evidence / Product

## Goal

After `T1a`, `T1b`, `G3`, and `P2a`, multiple surfaces share one **semantic kernel** (Trust Basis classification, Trust Card render, G3 predicate in `g3_authorization_context`, pack engine `1.2`, P2a MCP-001). **H1** hardens alignment so classifier, pack lint, Trust Card, CLI, and **release/migration truth** do not drift.

This is a **hardening wave**, not a capability wave.

## H1 does not redefine truth

- H1 adds **no** new trust claims, **no** new pack semantics, **no** new signal emitters, **no** new engine check types.
- H1 **aligns tests and documentation** to existing behavior; see [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) for contract SSOT.

## Single source of truth

**Primary SSOT (fixed filename):** [MIGRATION-TRUST-COMPILER-3.2.md](MIGRATION-TRUST-COMPILER-3.2.md) — Trust Card schema, claim contract (`claim.id` not position), engine version, pack floors, Trust Card invariants (frozen top-level keys; claims derived from Trust Basis only), release checklist, demo regeneration path.

This PLAN references that document; it does not duplicate full migration tables.

## Golden / demo bundles

| Strategy | Role |
|----------|------|
| **Regeneration path (default)** | Ignored test `write_mcp_lint_demo_bundles` in `mcp_signal_followup_pack.rs` + commands in migration SSOT. |
| **Committed minimal fixtures** | Only where already small and shared with existing tests; no large duplicate tarballs. |

## References

- [PLAN-T1a — Trust Basis Compiler](PLAN-T1a-TRUST-BASIS-COMPILER-2026q2.md)
- [PLAN-T1b — Trust Card](PLAN-T1b-TRUST-CARD-2026q2.md)
- [PLAN-G3 — Authorization context evidence](PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md)
- [PLAN-P2a — MCP signal follow-up pack](PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)
- [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md) §6 — sequencing
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md) — `g3_authorization_context_present`

## Acceptance (mechanical)

1. Migration SSOT exists and is linked from README, CHANGELOG (Unreleased), PLAN-P2a.
2. At least one integration test uses the **same bundle bytes** for Trust Basis + MCP-001 lockstep assertions.
3. At least one test asserts Trust Basis ↔ Trust Card: **same `claims` as Basis**, **frozen top-level keys** (`schema_version` / `claims` / `non_goals`), **no extra claim classification** in the card layer.
4. ROADMAP and RFC-005 place **H1 before P2b** explicitly.
