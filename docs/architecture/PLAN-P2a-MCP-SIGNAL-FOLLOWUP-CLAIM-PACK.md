# PLAN — P2a MCP Signal Follow-Up Claim Pack

- Status: **Shipped** on `main` (built-in pack `mcp-signal-followup`)
- Date: 2026-03-23
- Owner: Evidence / Product

## Goal

Productize G1/G2/G3 signals as a **small companion pack** (not baseline expansion): MCP-001 aligns with Trust Basis G3 via a **shared predicate** in `assay-evidence`; MCP-002/003 use existing YAML check types.

## Pack identity

| Field | Value |
|-------|--------|
| Name | `mcp-signal-followup` |
| Version | `1.0.0` |
| Rules | MCP-001, MCP-002, MCP-003 |

## Engine

- **Pack engine v1.2** (see `crates/assay-evidence/src/lint/packs/checks.rs` `ENGINE_VERSION`).
- MCP-001 sets `engine_min_version: "1.2"` for `g3_authorization_context_present`.

## Implementation

- **`src/g3_authorization_context.rs`**: Shared G3 v1 predicate; `trust_basis::classify_authorization_context` delegates to `bundle_satisfies_g3_authorization_context_visible`.
- **`CheckDefinition::G3AuthorizationContextPresent`**: YAML `type: g3_authorization_context_present`.
- **Tests**: `crates/assay-evidence/tests/mcp_signal_followup_pack.rs` — open/built-in parity; MCP-002/003 smoke tests; **MCP-001 alignment**: bundles for which Trust Basis sets `authorization_context_visible` to **`verified`** do **not** emit an MCP-001 finding; bundles for which that claim is **`absent`** **do** emit an MCP-001 finding (same synthetic bundles as Trust Basis tests — not a separate informal inverse).

## `assay_min_version` (pack `requires`)

Pack YAML uses `>=3.2.3` to track the first released Assay line with **G3 evidence + Trust Card schema 2 + seven claims** (git tag **v3.2.3** is the reference for that prerequisite substrate; it is not the “first tag that contains this pack”).

**Release truth for the built-in pack:** Tag **v3.2.3** does **not** include `mcp-signal-followup`, pack engine **v1.2**, or `g3_authorization_context_present` — those land with the **P2a** change. At **crates.io / GitHub release** time, either:

- bump the workspace version (e.g. to `3.2.4`) and set `assay_min_version` to **`>=3.2.4`** (or whatever version first tags P2a), **or**

- keep `>=3.2.3` but document in release notes that the **built-in** `mcp-signal-followup` is only present in binaries from the commit range that includes P2a (the `requires` field then expresses **evidence contract** floor, not pack feature floor).

Reviewers should confirm the chosen floor against the **first published** Assay version that embeds this pack.

## Non-goals

- Authorization validity, issuer trust, correlation-only rules, A2A, engine-wide DSL expansion.

## Semantics note

P2a adds **no new trust-claim classification** in Trust Basis: it **consumes** the existing G3 kernel in the pack executor. Wording like “no new classifier wave” means **no new claim keys or classification rules** beyond factoring shared G3 logic into `g3_authorization_context.rs`.

## References

- [PLAN-G3](PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md)
- [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md)
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md)
