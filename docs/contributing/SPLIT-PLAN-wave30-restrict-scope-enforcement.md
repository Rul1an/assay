# SPLIT PLAN — Wave30 Restrict Scope Enforcement

## Intent
Freeze a bounded runtime enforcement contract for `restrict_scope`, using the contract/evidence shape introduced in Wave29.

This wave is about:
- when `restrict_scope` is evaluated as enforceable
- how mismatch/missing/unsupported scope signals are handled
- which decision outcome applies
- which evidence fields are required for deny paths

It explicitly does **not** add:
- argument rewriting/filtering
- `redact_args` execution
- approval workflow changes
- control-plane semantics
- policy backend replacement
- auth transport changes

## Problem
Wave29 introduced a typed `restrict_scope` contract and additive evidence, but deliberately kept runtime behavior non-blocking.

Current gap:
- `restrict_scope` is present but not enforced
- mismatches do not deterministically deny yet
- deny-path evidence for scope enforcement is not frozen as a contract

## Frozen enforcement contract
Wave30 freezes `restrict_scope` runtime evaluation to these checks:

1. A `restrict_scope` obligation is present for the evaluated tool path.
2. Scope contract fields are available:
   - `scope_type`
   - `scope_value`
   - `scope_match_mode`
3. Scope evaluation result is interpreted deterministically:
   - `scope_evaluation_state=matched` => allow path may proceed
   - `scope_evaluation_state=mismatch` => deny
   - `scope_evaluation_state=not_evaluated` => deny

## Frozen failure handling
Wave30 freezes the following failure reasons as deny-path causes:

- `scope_target_missing`
- `scope_target_mismatch`
- `scope_match_mode_unsupported`
- `scope_type_unsupported`

Default outcome in this wave is bounded to `deny` for invalid/mismatched scope.
`deny_with_alert` is out of scope unless a later wave explicitly freezes it.

## Frozen evidence contract
Wave30 freezes additive evidence fields for `restrict_scope` enforcement outcomes.

Minimum required evidence fields:
- `scope_type`
- `scope_value`
- `scope_match_mode`
- `scope_evaluation_state`
- `scope_failure_reason`
- `restrict_scope_present`
- `restrict_scope_target`
- `restrict_scope_match`
- `restrict_scope_reason`

For deny paths, `scope_failure_reason` must be present and deterministic.

## Frozen semantics
### Valid restrict scope
A valid scope evaluation in Wave30 means:
- obligation is present
- scope target exists for the configured `scope_type`
- match-mode is supported
- evaluated result is `matched`

### Invalid restrict scope
Invalid means one of:
- missing scope target
- target mismatch
- unsupported match mode
- unsupported scope type

### Out of scope
This wave does not define:
- argument rewriting/filtering for scope correction
- grace/partial scope acceptance
- broad/global scope grants
- cross-session scope inheritance

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `restrict_scope` is runtime-enforced.
2. Mismatch/missing/unsupported scope conditions yield deterministic `deny`.
3. Existing `log`, `alert`, and `approval_required` behavior remains stable.
4. Scope evidence fields remain additive and backward-compatible.
5. No argument rewriting/filtering/redaction is introduced.

## Scope boundaries
### In scope
- runtime enforcement for `restrict_scope`
- deterministic deny handling for frozen failure reasons
- additive deny-path evidence for scope enforcement
- tests and reviewer gates for the above

### Out of scope
- argument rewrite/filter pipelines
- `redact_args` execution
- approval workflow expansion
- control-plane and external orchestration
- policy backend replacement

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- runtime `restrict_scope` enforcement
- deterministic deny on frozen failure reasons
- additive evidence updates

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay narrow and deterministic.

Primary failure modes:
- sneaking in rewrite/redaction behavior
- changing existing obligation behavior unintentionally
- emitting non-additive event schema changes
