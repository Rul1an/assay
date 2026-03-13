# SPLIT PLAN - Wave34 Fail-Closed Matrix Typing

## Intent
Freeze a bounded fail-closed matrix contract for MCP runtime enforcement paths.

This wave is about:
- typed fail-closed classification
- deterministic fallback mode selection
- additive decision/evidence fields for fail-closed outcomes
- replay-safe, machine-readable failure reasons

It explicitly does **not** add:
- new obligation types or policy language changes
- control-plane orchestration or external workflow integrations
- auth transport redesign
- broad incident management semantics
- big-bang evaluator replacement

## Problem
Wave24-Wave33 established typed decisions, obligation execution, and normalized outcomes.
Fail behavior is still spread across handler/runtime paths and not yet frozen as one typed contract.

Current gap:
- no frozen matrix for risk/class to fallback mode
- no frozen deterministic reason code surface for fail-closed paths
- no additive evidence fields dedicated to fail-closed classification
- replay/diff cannot consistently distinguish deny-by-policy vs deny-by-fail-closed fallback

## Frozen fail-closed matrix contract
Wave34 freezes the target matrix dimensions as:

- `tool_risk_class`
- `fail_closed_mode`
- `fail_closed_trigger`
- `fail_closed_applied`
- `fail_closed_error_code`

## Frozen risk classes
Wave34 freezes a bounded risk class surface:

- `high_risk`
- `low_risk_read`
- `default`

Risk classification must remain deterministic for a given tool/tool-class input.

## Frozen fallback modes
Wave34 freezes fallback mode values to:

- `fail_closed`
- `degrade_read_only`
- `fail_safe_allow`

Default policy for this wave:
- `high_risk` -> `fail_closed`
- `low_risk_read` -> `degrade_read_only`
- `default` -> `fail_closed`

## Frozen trigger baseline
Wave34 freezes a bounded trigger set:

- `policy_engine_unavailable`
- `context_provider_unavailable`
- `runtime_dependency_error`

## Frozen reason-code baseline
Wave34 freezes deterministic fail-closed reason codes:

- `fail_closed_policy_engine_unavailable`
- `fail_closed_context_provider_unavailable`
- `fail_closed_runtime_dependency_error`
- `degrade_read_only_runtime_dependency_error`

## Frozen compatibility rule
Wave34 remains additive:
- existing decision/event fields remain present
- existing deny reason fields remain parseable
- fail-closed fields are additive and optional at introduction
- no obligation execution behavior change is allowed in Step1

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. The fail-closed matrix dimensions are represented in code.
2. Risk class and fallback mode selection are deterministic.
3. Fail-closed evidence fields are emitted additively.
4. Existing allow/deny behavior remains stable except where matrix-typed fallback is explicitly applied.
5. Existing obligation execution paths stay bounded and deterministic.
6. Replay consumers can distinguish fail-closed fallback from policy denials.

## Scope boundaries
### In scope
- fail-closed matrix contract typing
- additive decision/evidence fields for fail-closed paths
- tests and reviewer gates for deterministic fallback semantics

### Out of scope
- obligation surface expansion
- policy backend replacement
- auth transport changes
- UI/control-plane features
- external incident/case integrations

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- matrix typing in runtime decision path
- deterministic fail-closed/degrade selection
- additive fail-closed evidence fields

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay contract-first and bounded.

Primary failure modes:
- implicit behavior shifts outside matrix-defined triggers
- using free-form strings instead of deterministic reason codes
- breaking existing event consumers while adding fail-closed fields
