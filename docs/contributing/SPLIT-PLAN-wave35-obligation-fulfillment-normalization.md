# SPLIT PLAN — Wave35 Obligation Fulfillment Normalization

## Intent
Freeze a bounded contract for normalized obligation fulfillment evidence across existing runtime paths.

This wave is about:
- one normalized `obligation_outcomes` shape
- deterministic `reason_code`
- deterministic `enforcement_stage`
- fixed `normalization_version`
- explicit semantic separation between:
  - `policy_deny`
  - `fail_closed_deny`
  - `obligation_skipped`
  - `obligation_applied`
  - `obligation_error`

It explicitly does **not** add:
- new obligation types
- policy-engine scope expansion
- UI/control-plane
- auth transport changes
- broad workflow semantics

## Problem
Wave24-34 landed typed decisions, event v2, obligations execution slices, approval/restrict/redact paths, and fail-closed matrix typing.

Current gap:
- fulfillment/evidence semantics can drift per path unless normalized under one explicit contract.

## Frozen normalization contract
Wave35 freezes the normalized fulfillment/evidence target shape for `obligation_outcomes`:
- `obligation_type`
- `status`
- `reason`
- `reason_code`
- `enforcement_stage`
- `normalization_version`

## Frozen deterministic semantics
### Deterministic `reason_code`
- equivalent failures must map to stable `reason_code`
- no path-specific ad-hoc reason code drift

### Deterministic `enforcement_stage`
- equivalent enforcement layers must emit stable stage labels
- no mixed stage labels for same logical layer

### Fixed `normalization_version`
- normalization version is explicitly emitted
- version drift must be intentional and reviewable

## Frozen separation model
Wave35 freezes explicit distinction between:
- `policy_deny`: denied by policy decision
- `fail_closed_deny`: denied by fail-closed fallback behavior
- `obligation_skipped`: obligation present but intentionally not applied
- `obligation_applied`: obligation successfully applied
- `obligation_error`: obligation attempted but errored

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:
1. Normalized fulfillment shape is represented additively.
2. `reason_code`, `enforcement_stage`, and `normalization_version` are deterministic for covered paths.
3. Separation of `policy_deny` vs `fail_closed_deny` is explicit in evidence semantics.
4. Existing behavior remains backward-compatible for current consumers.
5. No new obligation type is introduced.

## Scope boundaries
### In scope
- fulfillment normalization contract
- additive evidence contract
- tests and reviewer gates for the above

### Out of scope
- new obligation type implementation
- policy backend redesign
- UI/control-plane
- auth transport redesign

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- additive normalization/evidence contract updates
- deterministic semantics hardening for existing paths

### Step3
Docs + gate-only closure

## Reviewer notes
This wave must remain normalization-first.

Primary failure modes:
- adding new execution semantics under a normalization label
- conflating `policy_deny` and `fail_closed_deny`
- weakening determinism for `reason_code`, `enforcement_stage`, or `normalization_version`
