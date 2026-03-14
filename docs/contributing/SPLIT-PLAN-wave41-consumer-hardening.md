# SPLIT PLAN - Wave41 Consumer Hardening

## Intent
Freeze a bounded downstream consumer contract for decision and replay payloads before any further runtime capability work.

This wave is about:
- stable consumer-facing read semantics for `DecisionEvent` / `DecisionData` / `ReplayDiffBasis`
- deterministic read precedence across normalized and legacy payload shapes
- additive compatibility signaling for payload consumers
- reviewer gates that lock the consumer contract

It explicitly does **not** add:
- new runtime behavior
- new obligation types
- policy language expansion
- control-plane or auth transport changes
- UI or external integrations

## Problem
Waves 37 through 40 converged the runtime evidence model, replay basis, and deny-path evidence.

Current gap:
- downstream consumers still have to infer which payload shape to trust first
- read precedence across converged fields, replay compatibility markers, and legacy fields is not frozen as a dedicated contract
- consumer-oriented fallback signaling is not yet frozen independently from runtime normalization

## Frozen consumer contract
Wave41 freezes a consumer-facing read contract for these payload surfaces:
- `DecisionEvent`
- `DecisionData`
- `ReplayDiffBasis`

## Frozen read precedence
Wave41 freezes deterministic consumer read precedence:
1. normalized / converged decision fields
2. replay and compatibility markers
3. legacy base decision fields

This precedence is contract-level in Step1 and must remain deterministic.

## Frozen required consumer signals
Wave41 freezes the expectation that these signals remain available to downstream consumers:
- `decision`
- `reason_code`
- `decision_outcome_kind`
- `decision_origin`
- `fulfillment_decision_path`
- `decision_basis_version`

## Suggested additive markers for Step2
Wave41 Step2 may add consumer-facing compatibility metadata, additively only, such as:
- `decision_consumer_contract_version`
- `consumer_read_path`
- `consumer_fallback_applied`
- `consumer_payload_state`
- `required_consumer_fields`

Names can vary in Step2 implementation, but the semantics above are frozen in this plan.

## Frozen compatibility requirements
Wave41 freezes additive compatibility expectations for payload consumers:
- consumers can determine which payload path won without re-deriving runtime semantics
- fallback application is explicitly signaled for consumer-facing reads
- existing payload consumers are not broken by the hardening slice

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. consumer read precedence is explicit and testable
2. consumer-facing compatibility metadata is additive and backward-compatible
3. existing runtime decision behavior remains unchanged
4. replay/diff consumers can read payloads deterministically without bespoke precedence logic
5. no new runtime capability or policy surface is introduced

## Scope boundaries
### In scope
- decision/replay consumer contract freeze
- deterministic consumer read precedence contract
- additive consumer compatibility contract
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
- consumer-facing compatibility normalization
- deterministic read precedence representation
- additive consumer metadata
- no runtime behavior change

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain consumer-hardening only.

Primary failure modes:
- introducing runtime behavior changes under a consumer-compat label
- making read precedence less deterministic for downstream consumers
- breaking existing event/replay consumers via non-additive changes
