# SPLIT PLAN - Wave25 Obligations Log Execution

## Intent
Introduce a bounded Step1 freeze for Wave25 that turns Wave24 typed obligations into a minimal runtime execution slice.

This wave is about **single-obligation execution** with strict scope control.

It freezes:
- obligation execution scope to `log` only
- compatibility handling for `legacy_warning`
- additive decision evidence for obligation fulfillment

It explicitly does **not** add:
- `approval_required` enforcement
- `restrict_scope` enforcement
- `redact_args` enforcement
- auth transport changes
- policy backend changes

## Problem
Wave24 delivered typed decisions and Decision Event v2, but obligations are still contractual metadata.

Current gap:
- `allow_with_obligations` is represented in decisions/events
- no bounded runtime execution path exists yet
- no first-class fulfillment record is emitted

## Frozen execution contract (Wave25)
Wave25 freezes the first executable obligation scope as:
- `log`

Compatibility rule:
- `legacy_warning` is treated as compatibility input and executed as `log`
- existing `AllowWithWarning` parse and mapping behavior must remain intact

## Frozen fulfillment semantics
For each obligation attached to a decision, runtime must produce a deterministic outcome entry.

Frozen outcome states:
- `applied`
- `skipped`
- `error`

Wave25 policy for unknown/non-executable obligation types:
- do not block tool execution
- emit outcome as `skipped` with reason

## Frozen Decision Event addition
Decision Event payload is extended additively with:
- `obligation_outcomes`

Each outcome entry should minimally include:
- `type`
- `status`
- `reason` (optional)

All existing Decision Event v2 fields from Wave24 remain unchanged.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `allow_with_obligations` can execute `log` obligations in runtime paths.
2. `legacy_warning` compatibility path produces `log` execution outcomes.
3. Decision events include additive `obligation_outcomes`.
4. Existing allow/deny behavior remains stable.
5. Existing event consumers continue to parse events without breakage.
6. No approval/restrict/redact execution is introduced.

## Scope boundaries
### In scope
- MCP runtime obligation handling for `log`
- compatibility mapping for `legacy_warning`
- additive evidence field for fulfillment outcomes
- tests and reviewer gates needed for the above

### Out of scope
- approval-required runtime enforcement
- scope restriction runtime enforcement
- argument redaction runtime enforcement
- fail-closed matrix redesign
- policy backend/pluggable PDP work
- auth transport redesign

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- execute `log` obligations
- map `legacy_warning` to `log` execution outcome
- emit additive `obligation_outcomes`

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain narrow and additive.

Primary failure modes:
- accidental scope creep into high-risk obligations
- event compatibility regressions
- hidden behavior changes in existing allow/deny paths
