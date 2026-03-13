# SPLIT PLAN - Wave32 Redact Args Enforcement

## Intent
Freeze a bounded runtime enforcement contract for `redact_args`, using the typed contract and additive evidence shape introduced in Wave31.

This wave is about:
- when `redact_args` is enforceable at runtime
- how missing/unsupported redaction inputs are handled
- which decision outcome applies on invalid redaction requirements
- which evidence fields are required on deny paths

It explicitly does **not** add:
- broad/global scrub policy semantics
- PII detection engines
- external DLP integrations
- approval workflow changes
- `restrict_scope` behavior changes
- control-plane/auth transport changes

## Problem
Wave31 introduced `redact_args` as a typed, additive contract and kept it intentionally contract-only.

Current gap:
- `redact_args` is represented but not runtime-enforced
- invalid redaction requirements do not deterministically deny yet
- deny-path evidence for redaction enforcement is not frozen

## Frozen enforcement contract
Wave32 freezes `redact_args` runtime evaluation to these checks:

1. A `redact_args` obligation is present for the evaluated tool path.
2. Redaction contract fields are available:
   - `redaction_target`
   - `redaction_mode`
   - `redaction_scope`
3. Redaction evaluation state is interpreted deterministically:
   - `redaction_applied_state=applied` -> allow path may proceed
   - `redaction_applied_state=not_applied` -> deny
   - `redaction_applied_state=not_evaluated` -> deny

## Frozen failure handling
Wave32 freezes the following failure reasons as deny-path causes:

- `redaction_target_missing`
- `redaction_mode_unsupported`
- `redaction_scope_unsupported`
- `redaction_apply_failed`

Default outcome in this wave is bounded to `deny` for invalid/missing redaction requirements.
`deny_with_alert` is out of scope unless a later wave freezes it explicitly.

## Frozen evidence contract
Wave32 freezes additive evidence fields for `redact_args` enforcement outcomes.

Minimum required evidence fields:
- `redaction_target`
- `redaction_mode`
- `redaction_scope`
- `redaction_applied_state`
- `redaction_reason`
- `redaction_failure_reason`
- `redact_args_present`
- `redact_args_target`
- `redact_args_mode`
- `redact_args_result`
- `redact_args_reason`

For deny paths, `redaction_failure_reason` must be present and deterministic.

## Frozen semantics
### Valid redact_args
A valid redaction evaluation in Wave32 means:
- obligation is present
- target/mode/scope are supported
- evaluation result is `redaction_applied_state=applied`

### Invalid redact_args
Invalid means one of:
- missing redaction target
- unsupported redaction mode
- unsupported redaction scope
- redaction apply failure
- not applied/not evaluated for required redaction

### Out of scope
This wave does not define:
- broad/global org-wide redaction policies
- PII/classifier-driven implicit redaction
- external DLP orchestration
- cross-session redaction inheritance

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `redact_args` is runtime-enforced.
2. Missing/unsupported/not-applied redaction requirements yield deterministic `deny`.
3. Existing `log`, `alert`, `approval_required`, and `restrict_scope` behavior remains stable.
4. Redaction evidence fields remain additive and backward-compatible.
5. No broad/global redaction policy behavior is introduced.

## Scope boundaries
### In scope
- runtime enforcement for `redact_args`
- deterministic deny handling for frozen redaction failure reasons
- additive deny-path evidence for redaction enforcement
- tests and reviewer gates for the above

### Out of scope
- broad/global scrub policies
- PII detection engines
- external DLP integrations
- approval/restrict_scope semantics expansion
- policy backend replacement

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- runtime `redact_args` enforcement
- deterministic deny on frozen redaction failure reasons
- additive evidence updates

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay narrow and deterministic.

Primary failure modes:
- sneaking in broad/global redaction behavior
- changing existing obligation behavior unintentionally
- emitting non-additive event schema changes
