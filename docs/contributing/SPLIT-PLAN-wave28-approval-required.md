# SPLIT PLAN — Wave28 Approval Required Enforcement

## Intent
Freeze a bounded runtime enforcement contract for `approval_required`, using the approval artifact shape introduced in Wave27.

This wave is about:
- when approval is considered valid
- how missing / expired / mismatched approval is handled
- which decision outcome applies
- which evidence fields are required

It explicitly does **not** add:
- approval UI or case management
- external approval services
- control-plane semantics
- `restrict_scope` execution
- `redact_args` execution
- auth transport changes

## Problem
Wave27 introduced approval artifact/data shape and additive evidence fields, but did not enforce `approval_required` at runtime.

Current gap:
- no frozen runtime validity contract for approval artifacts
- no frozen handling for missing/expired approval
- no frozen mismatch semantics for `bound_tool` / `bound_resource`
- no frozen evidence contract for approval enforcement outcomes

## Frozen enforcement contract
Wave28 freezes `approval_required` runtime evaluation to these validity checks:

1. Approval artifact must be present.
2. Approval artifact must be fresh:
   - `issued_at` present
   - `expires_at` present
   - current evaluation time must be before expiry
3. Approval artifact must be correctly bound:
   - `bound_tool` must match the requested tool
   - `bound_resource` must match the requested resource when resource binding is present

## Frozen failure handling
Wave28 freezes missing/invalid approval behavior as:

- missing approval -> deny
- expired approval -> deny
- bound tool mismatch -> deny
- bound resource mismatch -> deny

This wave does **not** freeze `deny_with_alert` as default behavior for approval failures.
Default outcome in this wave is bounded to `deny` unless explicitly overridden in a later wave.

## Frozen evidence contract
Wave28 freezes additive evidence fields for approval enforcement outcomes.

Minimum required evidence fields:
- `approval_state`
- `approval_id` or `approval_ref`
- `approval_freshness`
- `approval_bound_tool`
- `approval_bound_resource`
- `approval_failure_reason` (for deny paths)

## Frozen semantics
### Valid approval
A valid approval artifact in Wave28 means:
- artifact exists
- artifact is not expired
- artifact is correctly bound to tool/resource scope

### Invalid approval
Invalid means one of:
- missing
- expired
- bound_tool mismatch
- bound_resource mismatch

### Out of scope
This wave does not define:
- broad/global approvals
- approval refresh workflows
- approval renewal
- partial approvals
- approval inheritance across sessions

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `approval_required` is enforced at runtime.
2. Missing/expired/mismatched approval yields deterministic `deny`.
3. Existing `log` and `alert` obligations remain stable.
4. Approval evidence fields remain additive and backward-compatible.
5. No UI/case-management/external approval integrations are introduced.
6. No `restrict_scope` or `redact_args` execution is introduced.

## Scope boundaries
### In scope
- runtime enforcement for `approval_required`
- approval validity checks
- additive evidence fields for approval enforcement
- tests and reviewer gates for the above

### Out of scope
- approval UI/case management
- external approval integrations
- control-plane work
- broad/global approval semantics
- `restrict_scope` execution
- `redact_args` execution

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- runtime enforcement of `approval_required`
- validity checks for presence / freshness / binding
- additive evidence for deny outcomes

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay narrow.

Primary failure modes:
- sneaking in broader approval workflow semantics
- breaking backward-compatible event consumers
- overloading approval failure with alerting/control-plane behavior too early
