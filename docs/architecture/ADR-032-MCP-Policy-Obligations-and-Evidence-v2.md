# ADR-032: MCP Policy Enforcement, Obligations, and Evidence v2

## Status
Accepted (March 2026)

## Context
Assay's MCP governance line started as deterministic pre-execution gating (allow/deny + taxonomy and sequence controls).
That foundation worked, but enterprise deployment required a broader runtime contract:

- typed decisions beyond binary allow/deny
- obligations as first-class runtime outcomes
- richer, replayable decision evidence
- explicit separation between identity/token issuance and policy enforcement

Without this, policy remained too narrow for approval/scope/redaction controls and audit replay across policy revisions.

## Decision
Assay evolves to a runtime policy enforcement and evidence layer for MCP tool calls.

### 1. Product role boundary
Assay does not become an IdP, OAuth server, or token broker.
Assay consumes auth context as policy input and stays above transport auth.

### 2. Architecture boundary
Assay uses an explicit PEP/PDP/PIP model:

- PEP: MCP runtime hook (`mcp wrap` / tool-call path)
- PDP: policy evaluator
- PIP: runtime context providers (auth summary, approval state, scope bindings, risk/time/quota inputs)
- Decision log: replayable evidence stream

### 3. Runtime decision contract
Decision output is typed and versioned:

- `allow`
- `allow_with_obligations`
- `deny`
- `deny_with_alert`

Compatibility path is preserved for legacy `AllowWithWarning` until migration is complete.

### 4. Obligations + evidence contract
Obligations are modeled and emitted as additive evidence.
Execution is introduced in bounded slices to avoid scope creep.

### 5. Transport/auth scope
HTTP and STDIO auth models are not collapsed into one transport contract.
Assay consumes context; it does not own token issuance or browser auth flows.

## Implementation Status (Closed on Main Through Wave42)
Delivered slices on `main`:

- Wave24: typed decisions + Decision Event v2 contract
- Wave25: `log` obligation execution
- Wave26: `alert` obligation execution
- Wave27: approval artifact/data shape + additive evidence
- Wave28: `approval_required` runtime enforcement (bounded deny semantics)
- Wave29: `restrict_scope` shape/evidence (no execution)
- Wave30: `restrict_scope` runtime enforcement
- Wave31: `redact_args` shape/evidence (no execution)
- Wave32: `redact_args` runtime enforcement + deterministic deny reasons
- Wave33: normalized `obligation_outcomes` fields (`reason_code`, `enforcement_stage`, `normalization_version`)
- Wave34: fail-closed matrix typing + additive fail-closed evidence
- Wave35: fulfillment normalization hardening across obligation paths
- Wave36: redact-enforcement hardening aligned with normalized fulfillment semantics
- Wave37: decision/evidence convergence across policy, fail-closed, enforcement, and obligation paths
- Wave38: replay diff basis + deterministic diff buckets
- Wave39: replay/evidence compatibility normalization for legacy and converged payloads
- Wave40: deny-evidence convergence + deterministic deny precedence
- Wave41: consumer read-precedence hardening for decision and replay payloads
- Wave42: context-envelope completeness hardening for `lane`, `principal`, `auth_context_summary`, and `approval_state`

For the current maintainer-facing architecture view, see the
[ADR-032 Implementation Overview](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md).
For the current structural decomposition and quality attributes, see the
[ADR-032 Building Block View](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md)
and [ADR-032 Quality Scenarios](./QUALITY-SCENARIOS-ADR-032-MCP-POLICY-STACK-2026q2.md).

## Short-Term Scope (What We Will Build)
Near-term follow-up remains limited to bounded hardening and consistency work around this contract:

- preserve additive compatibility paths while consumers move to normalized payloads
- keep replay/diff ergonomics deterministic as new readers are added
- keep lane/principal/auth summary envelope additive and explicit
- treat any new runtime capability as a separate bounded wave, not as incidental hardening

## Non-Goals (What We Will Not Build)
Not in scope for this ADR line:

- own OAuth/IdP/token issuance platform
- approval UI/case-management platform
- external incident/case orchestration as required runtime dependency
- broad control-plane rewrite
- big-bang policy engine migration

## Consequences
Positive:

- runtime policy decisions are stronger, typed, and audit-ready
- obligation execution is incremental and testable
- compatibility path avoids breaking existing event consumers
- replay and evidence contracts are now central product primitives

Tradeoffs:

- more policy/event surface area to maintain
- slower delivery by design due to bounded wave discipline
- temporary compatibility code paths remain until explicit deprecation waves complete

## Public API Note (Wave39/Wave40 Replay Evidence)
The replay/evidence hardening waves widened `assay_core::mcp::decision::ReplayDiffBasis`
with additional deny-convergence and compatibility fields.

This widening is intentional public API surface within the `v3.x` line, not an accidental
internal-only detail:

- downstream code that constructs `ReplayDiffBasis` directly must treat the new fields as part
  of the frozen replay basis contract
- downstream consumers that deserialize emitted replay basis data should expect these fields to
  be present after Wave39/Wave40 normalization
- no retroactive compatibility shim is introduced in this hygiene follow-up; any future external
  compatibility adapter should be handled as a separate, explicit follow-up
