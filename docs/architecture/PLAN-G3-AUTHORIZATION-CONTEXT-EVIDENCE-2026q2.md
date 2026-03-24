# PLAN — G3 Authorization Context Evidence (2026 Q2)

> Status: Implemented on `main` (March 2026)
> Scope: G3 v1 signal on `assay.tool.decision`, trust claim `authorization_context_visible`, Trust Card schema `2`.

## Goal

Emit a **bounded authorization-context** signal on policy-projected MCP decision evidence: `auth_scheme`, `auth_issuer`, and subject via existing `principal` — without token material, validation semantics, or trust scoring.

## v1 field set (frozen)

| Field | Rule |
|-------|------|
| `auth_scheme` | Allowlist only: `oauth2`, `jwt_bearer` (lowercase in JSON). Unknown values dropped at emit. |
| `auth_issuer` | Trimmed string; max 2048 bytes; no JWT dumps. |
| `principal` | Unicode-trimmed; whitespace-only treated as absent for G3. |

No `auth_subject`, no `auth_audience` in v1.

## Supported flow

Merge path: `ToolCallHandlerConfig.auth_context_projection` → `AuthContextProjection::merge_into_metadata` after `evaluate_with_metadata` (`crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`). Production callers pass `None` unless they supply policy-projected metadata.

## Trust compiler

- `TrustClaimId::AuthorizationContextVisible` / `TrustClaimBoundary::SupportedAuthProjectedFlowsOnly`.
- Claim order: after `delegation_context_visible`, before `containment_degradation_observed`.
- Classifier: `crates/assay-evidence/src/trust_basis.rs` — `verified` only when all three fields satisfy v1 rules on at least one `assay.tool.decision` event.

## Trust Card

- `TRUST_CARD_SCHEMA_VERSION = 2` (`crates/assay-evidence/src/trust_card.rs`).
- Renderer unchanged: one extra table row only; no new prose sections.

## Language contract

**May:** authorization **context** is **visible** in evidence for supported flows.

**Must not imply:** valid authorization, trustworthy token, verified issuer chain, sufficient scopes, correct authorization, temporal validity checked.

## Migration

Consumers must not rely on exactly six trust-basis claims; identify claims by `id`.

## References

- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
