# SPLIT PLAN — Wave36 Redact Args Enforcement Hardening

## Intent
Freeze a bounded hardening contract for `redact_args` runtime enforcement, aligned with Wave35 fulfillment normalization semantics.

This wave is about:
- deterministic `redact_args` enforcement semantics
- deterministic mapping for redaction failure reasons
- deterministic `reason_code`, `enforcement_stage`, and `normalization_version`
- additive evidence stability for redact enforcement outcomes

It explicitly does **not** add:
- new obligation types
- broad/global redact policy semantics
- PII detection engines
- external DLP integrations
- UI/control-plane/auth transport changes

## Problem
Wave31 introduced the `redact_args` contract/evidence shape and Wave32 introduced bounded runtime enforcement.
Wave35 introduced normalized obligation fulfillment semantics.

Current gap:
- redact enforcement determinism is present in code but not frozen in one hardening contract
- failure-class mapping and normalization semantics need explicit freeze-level guarantees
- replay/audit expectations should be contract-bound for future refactors

## Frozen enforcement hardening contract
Wave36 freezes `redact_args` runtime enforcement invariants as:

1. `redact_args` remains a runtime-enforced obligation path.
2. Invalid redaction requirements deterministically deny with `reason_code=P_REDACT_ARGS`.
3. Fine-grained deny reasons remain deterministic in evidence via `redaction_failure_reason`.

## Frozen failure classes
Wave36 freezes these failure classes as deterministic redaction enforcement outcomes:

- `redaction_target_missing`
- `redaction_mode_unsupported`
- `redaction_scope_unsupported`
- `redaction_apply_failed`

## Frozen normalization alignment
Wave36 freezes alignment with normalized fulfillment semantics:

- stable `reason_code`
- stable `enforcement_stage`
- stable `normalization_version`
- deterministic separation of applied vs denied redact paths in `obligation_outcomes`

## Frozen additive evidence contract
Wave36 freezes these redact evidence fields as additive and backward-compatible:

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

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `redact_args` enforcement remains deterministic for the frozen failure classes.
2. `reason_code`, `enforcement_stage`, and `normalization_version` stay deterministic.
3. Redact evidence fields remain additive and backward-compatible.
4. Existing `log`, `alert`, `approval_required`, and `restrict_scope` behavior remains stable.
5. No global redact policy or external DLP workflow is introduced.

## Scope boundaries
### In scope
- redact enforcement hardening contract
- deterministic normalization alignment for redact outcomes
- additive evidence stability checks
- tests and reviewer gates for the above

### Out of scope
- new obligation type implementation
- broad/global redact policy semantics
- PII detection engines
- external DLP integrations
- UI/control-plane or auth transport scope

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- tighten deterministic mapping/normalization guarantees for redact enforcement
- additive evidence compatibility hardening
- no scope expansion

### Step3
Docs + gate-only closure

## Reviewer notes
This wave must remain hardening-first.

Primary failure modes:
- re-opening redact semantics beyond the frozen failure classes
- weakening deterministic mapping guarantees
- introducing non-additive schema drift in redact evidence fields
