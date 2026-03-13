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

## Implementation Status (Closed on Main Through Wave32)
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

## Short-Term Scope (What We Will Build)
Near-term follow-up is limited to hardening and consistency work around this contract:

- unify obligation fulfillment evidence semantics across all obligation types
- tighten fail-closed matrix typing for high-risk classes
- deepen replay/diff ergonomics against policy revisions
- keep lane/principal/auth summary envelope additive and deterministic

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
