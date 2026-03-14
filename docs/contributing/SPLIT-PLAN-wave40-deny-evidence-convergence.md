# SPLIT PLAN - Wave40 Deny Evidence Convergence

## Intent
Freeze a bounded deny-path evidence convergence contract before any further runtime capability work.

This wave is about:
- explicit deny evidence separation across deny classes
- deterministic precedence for deny classification
- additive compatibility signaling for legacy payload shapes
- reviewer gates that lock the convergence contract

It explicitly does **not** add:
- new obligation types
- runtime behavior changes
- policy language expansion
- control-plane or auth transport changes
- UI or external integrations

## Problem
Wave39 normalized replay/evidence compatibility fields across broad decision shapes.

Current gap:
- deny-path evidence is still easy to read inconsistently across consumers
- precedence between deny classes is not frozen as a dedicated contract
- legacy deny payload fallback signaling is not frozen as deny-specific contract language

## Frozen deny evidence contract
Wave40 freezes deny evidence separation between:
- `policy_deny`
- `fail_closed_deny`
- `enforcement_deny`

## Frozen deny precedence contract
Wave40 freezes deterministic precedence for deny classification:
1. `decision_outcome_kind` (canonical)
2. `decision_origin` + deny context markers
3. `fulfillment_decision_path` fallback
4. legacy deny fallback from base `decision`

This precedence is contract-level in Step1 and must remain deterministic.

## Frozen compatibility requirements
Wave40 freezes additive legacy compatibility expectations for deny evidence:
- legacy payloads remain classifiable as deny/non-deny deterministically
- fallback application is explicitly signaled in evidence metadata
- no existing deny consumers are broken by the convergence slice

## Suggested additive markers for Step2
Wave40 Step2 may add deny-specific compatibility metadata, additively only, such as:
- `deny_precedence_version`
- `deny_classification_source`
- `deny_legacy_fallback_applied`
- `deny_convergence_reason`

Names can vary in Step2 implementation, but semantics above are frozen in this plan.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. deny evidence separation is explicit and testable (`policy_deny` / `fail_closed_deny` / `enforcement_deny`)
2. deny precedence is deterministic and test-covered
3. legacy deny fallback is additive and backward-compatible
4. existing runtime decision behavior remains unchanged
5. no new runtime capability or policy surface is introduced

## Scope boundaries
### In scope
- deny evidence convergence contract freeze
- deterministic deny precedence contract
- additive legacy compatibility contract for deny payloads
- tests and reviewer gates for the above

### Out of scope
- new obligation types
- runtime behavior changes
- policy language extension
- control-plane or auth transport changes
- UI/external integration work

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- deny-path convergence normalization
- deterministic precedence representation
- additive legacy compatibility metadata
- no runtime behavior change

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain convergence-only and additive.

Primary failure modes:
- introducing behavior changes under convergence scope
- reducing deny-path clarity in emitted evidence
- breaking older deny-event consumers via non-additive changes
