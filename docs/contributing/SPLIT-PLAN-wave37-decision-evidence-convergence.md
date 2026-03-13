# SPLIT PLAN — Wave37 Decision Evidence Convergence

## Intent
Freeze a bounded convergence contract for decision/evidence outcomes across existing MCP runtime paths.

This wave is about:
- one normalized outcome shape
- deterministic outcome classification across deny/apply/skip/error paths
- deterministic `reason_code`, `enforcement_stage`, and `normalization_version`
- additive compatibility rules for downstream event consumers

It explicitly does **not** add:
- new obligation types
- new runtime enforcement capabilities
- policy-engine backend expansion
- UI/control-plane/auth transport changes

## Problem
After Waves 24–36, Assay has multiple mature execution paths (`policy`, `fail_closed`, `approval_required`, `restrict_scope`, `redact_args`, and obligation fulfillment). The stack is correct, but evidence semantics can drift if each path evolves separately.

Current gap:
- no single frozen convergence contract for outcome semantics
- risk of divergence between deny classes and obligation fulfillment classes
- replay/audit consumers need a stable normalized classification contract

## Frozen convergence contract
Wave37 freezes a canonical normalized outcome taxonomy:

- `policy_deny`
- `fail_closed_deny`
- `enforcement_deny`
- `obligation_applied`
- `obligation_skipped`
- `obligation_error`

## Frozen classification semantics
Wave37 freezes deterministic classification mapping:

1. Policy-rule deny paths classify as `policy_deny`.
2. Fail-closed fallback deny paths classify as `fail_closed_deny`.
3. Enforcement-time denies (for example approval/scope/redaction checks) classify as `enforcement_deny`.
4. Obligation execution paths classify as one of:
   - `obligation_applied`
   - `obligation_skipped`
   - `obligation_error`

## Frozen additive evidence contract
Wave37 freezes additive convergence fields (without removing existing fields):

- `decision_outcome_kind`
- `decision_origin`
- `enforcement_stage`
- `reason_code`
- `normalization_version`
- `outcome_compat_state`

## Frozen downstream compatibility rules
Wave37 freezes compatibility expectations for event consumers:

- existing required decision/event fields must remain present
- convergence fields are additive and backward-compatible
- deterministic mapping from legacy outcome signals to `decision_outcome_kind`
- no consumer-facing break in existing replay/diff pipelines

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Canonical outcome taxonomy is represented in runtime/event contracts.
2. Classification is deterministic for policy/fail-closed/enforcement/obligation paths.
3. Convergence fields are additive and backward-compatible.
4. Existing behavior for `log`, `alert`, `approval_required`, `restrict_scope`, and `redact_args` remains stable.
5. No new runtime capability is introduced in this wave.

## Scope boundaries
### In scope
- normalized decision/evidence convergence contract
- deterministic classification mapping contract
- additive convergence evidence fields
- tests and reviewer gates for the above

### Out of scope
- new obligations or enforcement features
- policy backend expansion
- auth transport changes
- UI/control-plane semantics

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- add convergence fields/mapping in runtime/event contracts
- keep behavior additive and deterministic
- no capability expansion

### Step3
Docs + gate-only closure

## Reviewer notes
This wave must remain convergence-first.

Primary failure modes:
- introducing a new capability instead of normalizing existing outcomes
- weakening deterministic classification
- breaking downstream consumers by non-additive event changes
