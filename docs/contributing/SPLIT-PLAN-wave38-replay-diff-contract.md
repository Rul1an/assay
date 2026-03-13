# SPLIT PLAN - Wave38 Replay Diff Contract

## Intent
Freeze a bounded replay/diff contract for decision evidence before adding any new runtime capability.

This wave is about:
- deterministic replay comparison inputs
- deterministic diff classification buckets
- additive replay/diff evidence shape
- reviewer gates for contract lock-in

It explicitly does **not** add:
- new obligation types
- policy-engine backend changes
- control-plane workflows
- auth transport changes
- runtime enforcement semantics

## Problem
Wave37 converged decision/evidence classification at emission time.

Current gap:
- replay/diff comparisons are not frozen as a stable contract
- stricter/looser policy behavior is not normalized into deterministic buckets
- downstream evidence consumers have no frozen replay-diff basis

## Frozen replay basis
Wave38 freezes replay comparison basis to these fields:
- `decision_outcome_kind`
- `decision_origin`
- `outcome_compat_state`
- `fulfillment_decision_path`
- `reason_code`
- `typed_decision`
- `policy_version`
- `policy_digest`

## Frozen diff buckets
Wave38 freezes deterministic replay-diff bucket labels:
- `unchanged`
- `stricter`
- `looser`
- `reclassified`
- `evidence_only`

## Frozen semantics
### Unchanged
No change in effective decision/evidence outcome basis.

### Stricter
Replay outcome is strictly more restrictive than baseline.

### Looser
Replay outcome is strictly less restrictive than baseline.

### Reclassified
Decision remains effectively equivalent but shifts classification basis (e.g. policy deny vs fail-closed deny).

### evidence_only
Decision equivalence holds; only additive evidence fields differ.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Replay basis fields are represented in code-level comparison inputs.
2. Diff bucket mapping is deterministic and typed.
3. Existing event fields and runtime behavior stay stable.
4. Output is additive and backward-compatible for current consumers.
5. No new runtime enforcement capability is introduced.

## Scope boundaries
### In scope
- replay/diff contract freeze
- additive evidence shape freeze for replay outputs
- tests and reviewer gates for the above

### Out of scope
- new runtime obligations/enforcement
- policy backend replacements
- UI/control-plane workflows
- auth transport changes

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- typed replay basis
- typed diff buckets
- additive replay evidence output

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain contract-first and additive.

Primary failure modes:
- sneaking in runtime capability changes
- widening policy-engine scope
- making replay buckets non-deterministic
