# SPLIT PLAN - Wave39 Evidence Compatibility Normalization

## Intent
Freeze a bounded replay-facing evidence compatibility contract before any additional runtime capability is added.

This wave is about:
- replay-safe evidence shape hardening
- additive legacy fallback semantics
- deterministic classification precedence for replay consumers
- reviewer gates for contract lock-in

It explicitly does **not** add:
- new obligation types
- runtime enforcement changes
- policy language expansion
- control-plane or auth transport changes
- UI or external integrations

## Problem
Wave38 established replay diff basis and deterministic buckets.

Current gap:
- replay/evidence compatibility fields are not frozen as a dedicated contract
- legacy-shape fallback signaling is not normalized for downstream consumers
- classification provenance is not frozen for replay diagnostics

## Frozen compatibility contract
Wave39 freezes these replay/evidence compatibility concepts:
- `decision_basis_version`
- `compat_fallback_applied`
- `classification_source`
- `replay_diff_reason`
- `legacy_shape_detected`

## Frozen semantics
### decision_basis_version
Versioned identifier for the basis contract used to classify replay outcomes.

### compat_fallback_applied
Boolean signal that compatibility fallback logic was applied for this payload.

### classification_source
Deterministic source marker for classification precedence (for example: converged fields, fulfillment path, legacy fallback).

### replay_diff_reason
Deterministic reason token for replay-facing classification explanation.

### legacy_shape_detected
Boolean marker indicating legacy payload shape was detected during normalization.

## Classification precedence (frozen)
Wave39 freezes deterministic precedence order for replay/evidence compatibility classification:
1. converged outcome markers
2. fulfillment path markers
3. legacy fallback markers

This is a contract freeze only; behavior changes are out of scope for Step1.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Compatibility fields are represented additively in replay-facing evidence payloads.
2. Classification precedence is deterministic and testable.
3. Legacy fallback signaling remains backward-compatible.
4. Existing runtime behavior stays stable.
5. No new runtime enforcement capability is introduced.

## Scope boundaries
### In scope
- replay/evidence compatibility contract freeze
- additive compatibility field contract
- deterministic precedence contract
- tests and reviewer gates for the above

### Out of scope
- new obligation types
- runtime enforcement changes
- policy language extension
- control-plane or auth transport changes
- UI/external integration work

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- additive replay/evidence compatibility fields
- deterministic precedence representation
- no runtime capability expansion

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay compatibility-first and additive.

Primary failure modes:
- introducing runtime behavior changes under a compatibility label
- breaking legacy event consumers
- widening scope into policy/runtime engine changes
