# SPLIT PLAN - Wave26 Obligations Alert Execution

## Intent
Introduce a bounded Step1 freeze for Wave26 that extends the Wave25 runtime obligations path with one additional low-risk obligation type.

This wave is about **incremental obligation execution** with strict scope control.

It freezes:
- runtime execution scope to `log` + `alert`
- compatibility handling for `legacy_warning` -> `log`
- additive fulfillment evidence via existing `obligation_outcomes`

It explicitly does **not** add:
- `approval_required` enforcement
- `restrict_scope` enforcement
- `redact_args` enforcement
- auth transport changes
- policy backend changes

## Problem
Wave25 delivered bounded runtime execution for `log`, but one additional low-risk obligation path is still missing before moving to higher-risk obligations.

Current gap:
- `allow_with_obligations` can execute `log`
- no first-class runtime execution path exists yet for `alert`
- no explicit freeze exists for this intermediate scope

## Frozen execution contract (Wave26)
Wave26 freezes executable obligation scope as:
- `log`
- `alert`

Compatibility rule remains:
- `legacy_warning` is treated as compatibility input and executed as `log`
- existing `AllowWithWarning` parse and mapping behavior must remain intact

## Frozen `alert` semantics
`alert` execution in Wave26 is explicitly bounded:
- non-blocking for tool execution
- emits deterministic fulfillment outcome (`applied` / `skipped` / `error`)
- does not introduce external incident/case-management dependencies in this wave

## Frozen evidence semantics
Decision Event continues to use additive `obligation_outcomes` introduced in Wave25.

Each outcome entry remains minimally:
- `type`
- `status`
- `reason` (optional)

No new Decision Event field is required for Wave26 Step1 freeze.

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Runtime can execute `alert` obligations in the same bounded path as `log`.
2. `legacy_warning` compatibility path still produces `log` outcomes.
3. `obligation_outcomes` remains additive and backward-compatible.
4. Existing allow/deny behavior remains stable.
5. Existing event consumers continue to parse events without breakage.
6. No approval/restrict/redact execution is introduced.

## Scope boundaries
### In scope
- MCP runtime obligation handling for `alert`
- preserving existing `log` and `legacy_warning` behavior
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
- execute `alert` obligations
- preserve existing `log` + `legacy_warning` behavior
- keep evidence additive via existing `obligation_outcomes`

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain narrow and additive.

Primary failure modes:
- accidental scope creep into high-risk obligations
- event compatibility regressions
- hidden behavior changes in existing allow/deny paths
