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
- **Tests**: `crates/assay-evidence/tests/mcp_signal_followup_pack.rs` — open/built-in parity, Trust Basis verified/absent ↔ MCP-001 pass/fail, MCP-002/003 smoke tests.

## `assay_min_version` (pack `requires`)

Set to `>=3.2.3` in the pack YAML (first workspace line that ships G3 + Trust Card schema 2). Adjust at release if the floor changes.

## Non-goals

- Authorization validity, issuer trust, correlation-only rules, A2A, engine-wide DSL expansion.

## References

- [PLAN-G3](PLAN-G3-AUTHORIZATION-CONTEXT-EVIDENCE-2026q2.md)
- [RFC-005](RFC-005-trust-compiler-mvp-2026q2.md)
- [SPEC-Pack-Engine-v1](SPEC-Pack-Engine-v1.md)
