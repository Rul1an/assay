# SPLIT PLAN — Wave29 Restrict Scope Contract

## Intent
Freeze a bounded contract for `restrict_scope` before any runtime execution semantics are introduced.

This wave is about:
- restrict-scope artifact/data shape
- scope matching semantics
- additive decision-event/evidence fields
- compatibility boundaries for existing obligation behavior

It explicitly does **not** add:
- runtime blocking/enforcement for `restrict_scope`
- policy backend changes
- approval workflow changes
- `redact_args` execution
- external incident/case-management integrations

## Problem
Wave24–Wave28 established typed decisions, event v2, and bounded obligation execution (`log`, `alert`) plus `approval_required` enforcement.

Current gap:
- `restrict_scope` lacks a first-class frozen contract
- no frozen semantics for what constitutes scope match/mismatch
- no frozen additive evidence shape for scope outcomes

## Frozen restrict_scope contract
Wave29 freezes a minimum `restrict_scope` contract with:

- `scope_profile`
- `allowed_servers`
- `allowed_tool_classes`
- `allowed_resources`
- `max_resource_selectors`

The contract is policy/output-facing and does not imply execution in this wave.

## Frozen semantics
### Scope match basis
A request can be evaluated against:
- target `server_id`
- matched `tool_classes`
- requested `resource` selectors

### Scope mismatch basis
Mismatch conditions are frozen as named reasons:
- `scope_server_mismatch`
- `scope_tool_class_mismatch`
- `scope_resource_mismatch`
- `scope_selector_limit_exceeded`

These are frozen as semantics/evidence markers only for this wave.

## Frozen evidence contract
Wave29 freezes additive scope-related evidence fields:

- `scope_decision`
- `scope_effective`
- `scope_violation_reason`
- `scope_profile`
- `scope_policy_version`
- `scope_policy_digest`

Evidence additions must remain backward-compatible.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Restrict-scope contract shape is represented in code.
2. Scope match/mismatch semantics are represented deterministically.
3. Scope evidence fields are additive and backward-compatible.
4. Existing `log`/`alert`/`approval_required` behavior remains stable.
5. No runtime enforcement for `restrict_scope` is introduced yet.

## Scope boundaries
### In scope
- `restrict_scope` contract freeze
- additive evidence shape freeze
- tests and reviewer gates for the contract freeze

### Out of scope
- runtime enforcement/execution of `restrict_scope`
- `redact_args` execution
- approval workflow expansion
- policy backend replacement
- control-plane work
- auth transport changes

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- contract/data shape representation
- additive evidence representation
- no runtime enforcement

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay contract-first and additive.

Primary failure modes:
- introducing runtime `restrict_scope` enforcement too early
- breaking event consumers with non-additive schema changes
- scope-creep into approval/control-plane concerns
