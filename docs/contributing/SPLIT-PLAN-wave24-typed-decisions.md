# SPLIT PLAN — Wave24 Typed Decisions and Decision Event v2

## Intent
Introduce a bounded, backward-compatible contract upgrade for MCP runtime decisions and decision events.

This wave is about **contract shape**, not execution semantics.

It freezes:
- a typed decision model
- Decision Event v2 fields
- the compatibility path for `AllowWithWarning`

It explicitly does **not** add:
- obligations execution
- approval enforcement
- new policy backends
- transport auth changes
- lane control-plane features

## Problem
Current Assay MCP runtime decisions are still effectively:

- `Allow`
- `AllowWithWarning`
- `Deny`

This is too small for the next product step:
- obligations are not first-class
- warning semantics are underspecified
- decision evidence is useful but not yet version-complete
- replay/diff across policy revisions needs richer event context

## Frozen decision contract
Wave24 freezes the target logical model as:

- `allow`
- `allow_with_obligations`
- `deny`
- `deny_with_alert`

## Compatibility rule
Existing `AllowWithWarning` behavior must remain backward-compatible in this wave.

Frozen compatibility rule:
- existing `AllowWithWarning` remains parseable and usable
- implementation may internally map it to `allow_with_obligations`
- warning metadata must remain available as:
  - obligation metadata
  - and/or explicit compatibility fields in the emitted event payload

No silent semantic drift is allowed.

## Frozen Decision Event v2 fields
Decision Event v2 must add, at minimum:

- `policy_version`
- `policy_digest`
- `obligations`
- `approval_state`
- `lane`
- `principal`
- `auth_context_summary`

Existing fields must remain present:
- `tool`
- `tool_classes`
- `matched_tool_classes`
- `match_basis`
- `matched_rule`
- `reason_code`

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Existing allow/deny behavior stays stable for current tests.
2. `AllowWithWarning` remains backward-compatible.
3. Decision Event v2 contains the new frozen fields.
4. Existing required decision fields remain intact.
5. CLI consumers that normalize/consume decision events continue to pass.
6. No obligations execution is introduced in this wave.

## Scope boundaries
### In scope
- MCP policy/runtime decision contract
- MCP decision event schema
- compatibility path for `AllowWithWarning`
- tests and reviewer gates needed for the above

### Out of scope
- obligations execution
- approval artifact enforcement
- lane control-plane semantics
- Cedar/OPA backend work
- auth transport redesign
- fail-closed matrix behavior changes beyond explicit contract fields

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- typed decision contract
- Decision Event v2
- compatibility path for `AllowWithWarning`

### Step3
Docs + gate only closure

## Reviewer notes
This wave should remain additive and replay-friendly.

The highest-risk failure modes are:
- breaking existing event consumers
- drifting `AllowWithWarning` semantics
- accidentally introducing obligations execution
- changing CLI normalization behavior
