# SPLIT PLAN — Wave31 Redact Args Contract and Evidence Freeze

## Intent
Freeze a bounded `redact_args` contract and additive evidence shape before any runtime argument redaction is introduced.

This wave is about:
- typed `redact_args` obligation shape
- redactable argument zones
- additive evidence fields for redaction evaluation
- compatibility and reviewer gates

It explicitly does **not** add:
- runtime arg redaction execution
- mutation of tool payloads in execution paths
- broad/global scrub policies
- PII detection engines
- external DLP integrations
- UI/control-plane/auth transport changes

## Problem
Wave24–Wave30 established typed decisions, obligations, and enforcement for `log`, `alert`, `approval_required`, and `restrict_scope`.

Current gap:
- `redact_args` has no frozen typed contract
- redactable zones are not frozen
- additive evidence fields are not frozen
- execution semantics could drift without a contract-first step

## Frozen redact_args contract
Wave31 freezes the target typed obligation shape with, at minimum:

- `redaction_target`
- `redaction_mode`
- `redaction_scope`
- `redaction_applied_state`
- `redaction_reason`

## Frozen redactable zones
Wave31 freezes redactable argument zones as contract inputs only.

Target zones in this wave:
- request-like args fields (e.g. `path`, `query`, `headers`, `body`, `metadata`)
- scalar and structured argument fields explicitly addressed by `redaction_target`

Out of scope in this wave:
- runtime payload mutation
- implicit broad/global zone expansion
- automatic detection-driven targeting

## Frozen evidence contract
Wave31 freezes additive evidence fields for `redact_args` evaluation.

Minimum additive evidence fields:
- `redact_args_present`
- `redact_args_target`
- `redact_args_mode`
- `redact_args_result`
- `redact_args_reason`

Evidence must stay additive and backward-compatible.

## Frozen semantics
### Contract-only in Step1/Step2 of this wave
- `redact_args` may be represented and evaluated for evidence
- no runtime argument mutation may occur
- no execution-time rewriting/scrubbing may occur

### Out of scope
This wave does not define:
- runtime redaction side-effects
- broad/global organization-wide scrub policies
- PII classification logic
- external DLP workflow coupling

## Acceptance shape for Step2
A Step2 implementation is acceptable only if all of the following are true:

1. Typed `redact_args` obligation shape exists in code.
2. Redactable zones are explicitly represented as contract data.
3. Additive redaction evidence fields are emitted/available.
4. Existing runtime enforcement behavior remains stable.
5. No runtime payload mutation/redaction execution is introduced.

## Scope boundaries
### In scope
- typed `redact_args` shape
- redactable-zone contract surface
- additive evidence shape
- tests and reviewer gates for the above

### Out of scope
- runtime redaction execution
- payload rewriting/mutation
- broad/global scrub policies
- PII detection engines
- external DLP integrations
- UI/control-plane work

## Planned wave structure
### Step1
Docs + gate only

### Step2
Bounded implementation:
- typed `redact_args` contract representation
- additive evidence field support
- no execution semantics

### Step3
Docs + gate only closure

## Reviewer notes
This wave must remain contract-first and additive.

Primary failure modes:
- sneaking in runtime redaction behavior
- widening to global scrub policy semantics
- breaking existing event consumers with non-additive schema changes
