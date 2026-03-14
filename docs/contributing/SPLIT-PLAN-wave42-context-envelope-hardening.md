# SPLIT PLAN - Wave42 Context Envelope Hardening

## Intent
Freeze a bounded downstream context-envelope contract for decision payloads before any further runtime capability work.

This wave is about:
- stable consumer-facing semantics for `lane`, `principal`, `auth_context_summary`, and `approval_state`
- deterministic context-envelope completeness signaling for downstream readers
- additive context-contract metadata for emitted decision payloads
- reviewer gates that lock the context envelope contract

It explicitly does **not** add:
- new runtime behavior
- new obligation types
- policy language expansion
- control-plane or auth transport changes
- UI or external integrations

## Problem
The runtime now emits context fields such as `lane`, `principal`, `auth_context_summary`, and `approval_state`,
but that envelope is not yet frozen as a dedicated downstream contract.

Current gap:
- downstream consumers cannot distinguish complete envelope data from partial envelope data without bespoke field checks
- context completeness and payload robustness are not frozen independently from runtime decision semantics
- additive context-contract metadata is not yet defined as a bounded compatibility layer

## Frozen context envelope contract
Wave42 freezes a consumer-facing context envelope for these payload surfaces:
- `DecisionEvent`
- `DecisionData`

## Frozen context fields
Wave42 freezes the expectation that these fields remain available as the core context envelope:
- `lane`
- `principal`
- `auth_context_summary`
- `approval_state`

## Frozen completeness semantics
Wave42 freezes deterministic downstream semantics for context-envelope completeness:
1. complete envelope
2. partial envelope
3. absent envelope

This completeness classification is contract-level in Step1 and must remain deterministic.

## Suggested additive markers for Step2
Wave42 Step2 may add context-facing contract metadata, additively only, such as:
- `decision_context_contract_version`
- `context_payload_state`
- `required_context_fields`
- `missing_context_fields`

Names can vary in Step2 implementation, but the semantics above are frozen in this plan.

## Frozen compatibility requirements
Wave42 freezes additive compatibility expectations for context-envelope consumers:
- consumers can determine whether the envelope is complete, partial, or absent without re-deriving runtime semantics
- existing payload consumers are not broken by the hardening slice
- missing optional context does not introduce runtime behavior change in this wave

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. context-envelope completeness is explicit and testable
2. context-facing contract metadata is additive and backward-compatible
3. existing runtime decision behavior remains unchanged
4. downstream readers can reason about envelope completeness deterministically
5. no new runtime capability or policy surface is introduced

## Scope boundaries
### In scope
- decision context-envelope contract freeze
- deterministic completeness semantics for context fields
- additive context compatibility contract
- tests and reviewer gates for the above

### Out of scope
- runtime behavior changes
- new obligation types
- policy language extension
- control-plane or auth transport changes
- UI/external integration work

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- context-envelope completeness normalization
- additive context contract metadata
- no runtime behavior change

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain context-envelope hardening only.

Primary failure modes:
- introducing runtime behavior changes under a context-hardening label
- making envelope completeness less deterministic for downstream consumers
- breaking existing event consumers via non-additive changes
