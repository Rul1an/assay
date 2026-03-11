# SPLIT PLAN — Wave27 Approval Artifact Contract

## Intent
Freeze a bounded contract for approval artifacts before any runtime enforcement of `approval_required`.

This wave is about:
- approval artifact shape
- freshness / expiry semantics
- binding to tool/resource
- additive decision-event/evidence fields

It explicitly does **not** add:
- runtime blocking on missing approval
- approval UI or case management
- external approval services
- `restrict_scope` or `redact_args` execution
- policy backend changes

## Problem
Wave24 introduced typed decisions and Decision Event v2.
Wave25 and Wave26 introduced bounded execution for `log` and `alert`.

Before `approval_required` can become executable, the approval artifact itself must be frozen as a stable contract.

Current gap:
- no first-class frozen approval artifact schema
- no frozen freshness/expiry contract
- no frozen binding rules for tool/resource
- no additive evidence contract for approval state

## Frozen approval artifact contract
Wave27 freezes an approval artifact with, at minimum:

- `approval_id`
- `approver`
- `issued_at`
- `expires_at`
- `scope`
- `bound_tool`
- `bound_resource`

## Frozen semantics
### Freshness / expiry
- approval artifacts must be explicitly time-bounded
- expiry is part of the artifact contract
- no implicit infinite approval is allowed in the target model

### Binding
- approval must be bindable to:
  - a specific tool
  - a specific resource
- broad/global approval semantics are out of scope for this wave

### Evidence shape
Decision/event evidence may add approval-related fields, but only additively.
This wave freezes the target fields without enforcing them yet.

Minimum target evidence fields:
- `approval_state`
- `approval_id` (or `approval_ref`)
- `approval_freshness`
- `approval_bound_tool`
- `approval_bound_resource`

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Approval artifact shape is represented in code.
2. Approval freshness/expiry fields are represented.
3. Tool/resource binding fields are represented.
4. Event/evidence fields are additive and backward-compatible.
5. No runtime enforcement of `approval_required` is introduced yet.
6. Existing typed decision and obligations behavior remains stable.

## Scope boundaries
### In scope
- approval artifact contract
- additive event/evidence shape
- tests and reviewer gates for the above

### Out of scope
- runtime enforcement of `approval_required`
- approval execution flow
- UI/case-management
- external approval integrations
- control-plane semantics
- auth transport changes

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- artifact/data shape
- additive event/evidence fields
- no enforcement

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay contract-first.

Primary failure modes:
- sneaking in approval enforcement early
- breaking existing event consumers
- making approval semantics too broad
