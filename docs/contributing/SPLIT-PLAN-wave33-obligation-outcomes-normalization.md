# SPLIT PLAN - Wave33 Obligation Outcomes Normalization

## Intent
Freeze a bounded normalization contract for `obligation_outcomes` so runtime evidence stays stable across waves.

This wave is about:
- canonical `obligation_outcomes` shape
- deterministic status semantics
- normalized reason coding for replay/audit
- additive compatibility for existing event consumers

It explicitly does **not** add:
- new obligation execution types
- policy decision model changes
- approval/restrict/redact semantics expansion
- control-plane or external workflow integrations
- auth transport changes

## Problem
Wave24-Wave32 expanded typed decisions and obligation handling. The current `obligation_outcomes` stream is useful, but reason text is still mixed between:
- free-form human strings
- wave-specific phrasing
- partially normalized failure reasons

Current gap:
- no frozen normalization version for `obligation_outcomes`
- no frozen reason-code contract for applied/skipped/error paths
- mixed reason text makes replay/diff noisier than necessary

## Frozen normalization contract
Wave33 freezes the target normalized `obligation_outcomes` contract as:

- `obligation_type`
- `status`
- `reason` (human-readable compatibility field)
- `reason_code` (normalized machine-friendly field, additive)
- `enforcement_stage` (additive provenance field)
- `normalization_version` (additive schema marker)

## Frozen status semantics
Wave33 freezes status values to:

- `applied`
- `skipped`
- `error`

Status meaning must stay deterministic:
- `applied`: obligation path completed successfully for this attempt
- `skipped`: obligation intentionally not executed in this path
- `error`: obligation was required in-path but failed validation/enforcement

## Frozen reason-code surface
Wave33 freezes a bounded reason-code baseline:

- `legacy_warning_mapped`
- `validated_in_handler`
- `contract_only`
- `unsupported_obligation_type`
- `approval_missing`
- `approval_expired`
- `approval_bound_tool_mismatch`
- `approval_bound_resource_mismatch`
- `scope_target_missing`
- `scope_target_mismatch`
- `scope_match_mode_unsupported`
- `scope_type_unsupported`
- `redaction_target_missing`
- `redaction_mode_unsupported`
- `redaction_scope_unsupported`
- `redaction_apply_failed`

This wave keeps `reason` for backward compatibility while adding `reason_code` as the normalized field.

## Frozen compatibility rule
Wave33 remains additive:
- existing `obligation_outcomes` fields stay present
- existing event consumers continue to parse current payloads
- normalization fields are additive and optional at introduction
- no decision behavior changes are allowed in this wave

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. `obligation_outcomes` normalization fields are represented in code.
2. Existing `obligation_type/status/reason` compatibility is preserved.
3. Normalized `reason_code` is emitted for frozen baseline cases.
4. Existing allow/deny outcomes remain behaviorally unchanged.
5. Existing obligation execution scope remains unchanged.
6. Event/evidence changes are additive and replay-safe.

## Scope boundaries
### In scope
- obligation outcomes normalization contract
- additive event/evidence shape for normalized reasons
- tests and reviewer gates for normalization behavior

### Out of scope
- new obligation execution semantics
- approval workflow expansion
- restrict/redact contract redesign
- backend/policy language replacement
- control-plane integrations

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- additive normalization fields
- deterministic reason-code emission
- compatibility-preserving outcome shape

### Step3
Docs + gate only closure

## Reviewer notes
This wave must stay contract-first and additive.

Primary failure modes:
- changing runtime decisions while normalizing evidence
- dropping compatibility fields relied on by consumers
- introducing non-deterministic reason mapping
