# PLAN - P41 OpenFeature EvaluationDetails Decision Receipt Import

**Status:** execution slice
**Target repo:** `Rul1an/assay`
**Depends on:** P30, P31-P34
**Date:** 2026-04-27

## One-line goal

Turn one bounded OpenFeature boolean `EvaluationDetails` artifact into one
portable Assay decision receipt, without importing provider, context, rule, or
application truth.

## 1. Why this slice exists

P31 proved that a selected Promptfoo eval outcome can become a portable Assay
receipt. P41 opens a second family: runtime decision receipts.

OpenFeature is a good next wedge because `EvaluationDetails` is a small named
public result surface. It can carry a flag key, returned value, optional
variant, optional reason, and optional error code without requiring Assay to
understand provider configuration, targeting rules, evaluation context, or
application behavior.

This is not an OpenFeature integration. It is an Assay-side compiler lane over
one bounded decision-detail artifact.

## 2. Layering

The stack boundary is:

```text
OpenFeature SDK/provider
  -> returned EvaluationDetails<boolean>
  -> bounded OpenFeature EvaluationDetails artifact JSONL
  -> assay evidence import openfeature-details
  -> Assay EvidenceEvent receipt bundle
  -> evidence verify / trust-basis generate
```

Assay owns the receipt reduction and bundle semantics. OpenFeature remains the
decision API context, not the evidence or audit layer.

Harness is intentionally out of scope for P41. Harness may later gate/report
Trust Basis diffs over these receipts if a Trust Basis claim is added in a
separate slice.

## 3. Scope

P41 v1 imports exactly one bounded path:

- one JSONL row
- one `openfeature.evaluation-details.export.v1` artifact
- one boolean decision result
- one Assay EvidenceEvent receipt

The importer is boolean-only. String, number, object, and structured flag values
are follow-up lanes.

## 4. Input surface

P41 consumes the P30 bounded artifact shape, one JSON object per JSONL line:

```json
{
  "schema": "openfeature.evaluation-details.export.v1",
  "framework": "openfeature",
  "surface": "evaluation_details",
  "target_kind": "feature_flag",
  "flag_key": "checkout.new_flow",
  "result": {
    "value": true,
    "variant": "on",
    "reason": "STATIC",
    "error_code": null
  }
}
```

This is not a claim that OpenFeature has a single cross-SDK JSON wire shape.
The JSONL artifact is the downstream, reviewer-safe export shape derived from a
returned `EvaluationDetails<boolean>` object.

## 5. Receipt v1 thesis

The receipt body is an Assay EvidenceEvent payload:

```json
{
  "schema": "assay.receipt.openfeature.evaluation_details.v1",
  "source_system": "openfeature",
  "source_surface": "evaluation_details.boolean",
  "source_artifact_ref": "openfeature-details.jsonl",
  "source_artifact_digest": "sha256:...",
  "reducer_version": "assay-openfeature-evaluation-details@0.1.0",
  "imported_at": "2026-04-27T12:00:00Z",
  "decision": {
    "flag_key": "checkout.new_flow",
    "value_type": "boolean",
    "value": true,
    "variant": "on",
    "reason": "STATIC",
    "error_code": "FLAG_NOT_FOUND"
  }
}
```

Optional fields are omitted when absent or null. The receipt remains a bounded
decision receipt. It does not become a flag configuration, targeting, or
provider truth record.

## 6. Field rules

`decision.flag_key` names the evaluated flag key only. It is not provider
identity, user identity, or rollout truth.

`decision.value` is the boolean value returned by detailed evaluation. It is
not application correctness or provider correctness.

`decision.variant` is optional reviewer support when naturally present.

`decision.reason` is a bounded string. It may contain known OpenFeature reasons
such as `STATIC`, `DEFAULT`, `TARGETING_MATCH`, or `ERROR`, but Assay does not
treat it as a closed enum or global ontology.

`decision.error_code` is optional bounded machine support. P41 v1 excludes
`error_message` because it is free text and can become provider-specific or
leaky.

## 7. Strict exclusions

P41 v1 rejects:

- evaluation context
- targeting key or user identifiers
- targeting rules and segments
- provider configuration or provider state
- provider metadata
- inline flag metadata
- flag definitions and rollout configuration
- hooks and telemetry
- `error_message`
- bulk flag state or arrays of details

The importer should fail closed rather than silently discard a larger artifact.

## 8. Event type

P41 introduces an experimental Assay Evidence event type:

```text
assay.receipt.openfeature.evaluation_details.v1
```

Experimental means explicitly invoked importer only. It must not be emitted by
default evidence export paths.

## 9. Trust Basis posture

P41 does not add a Trust Basis claim. The first slice proves:

- the importer writes a verifiable evidence bundle
- the event type is registered
- the receipt payload is bounded
- the current Trust Basis compiler can read the bundle
- OpenFeature decision receipts are not classified as external eval receipts

A future slice may add a decision-receipt Trust Basis claim, but that should be
a separate compatibility decision.

## 10. Acceptance criteria

- `assay evidence import openfeature-details` exists
- valid boolean decision JSONL produces a verifiable evidence bundle
- each JSONL row produces exactly one receipt event
- non-boolean values fail closed
- context, metadata, provider, rules, and `error_message` fields fail closed
- receipt payload excludes raw context, provider, metadata, and free-text error
  messages
- CLI docs describe the boundary and Trust Basis posture
- the Evidence Contract registry lists the experimental event type

## 11. Non-goals

P41 does not:

- add OpenFeature provider support
- add an official OpenFeature integration
- parse provider configuration
- import flag definitions
- import evaluation context
- support all value types
- add a Trust Basis claim
- add Harness gates or reports
- claim that a flag decision was correct

## 12. Follow-ups

Possible follow-ups, only after P41 lands cleanly:

- P42 decision receipt Trust Basis claim
- Harness fixture/recipe over decision receipts
- string/number EvaluationDetails lanes
- a short Assay-side note: "From OpenFeature EvaluationDetails to Decision
  Receipts"
