# PLAN — P30 OpenFeature EvaluationDetails Evidence

- **Date:** 2026-04-25
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Explore one bounded OpenFeature-adjacent
  evidence lane built around a single `EvaluationDetails` object returned by a
  public detailed feature flag evaluation call. This plan is for one
  governance-adjacent decision-detail artifact only. It does not propose broad
  OpenFeature support, provider support, flag configuration import, targeting
  rule import, rollout import, telemetry import, or application correctness
  truth.

## 1. Why this plan exists

OpenFeature is a strong P30 candidate because it moves the interop queue out of
LLM evaluator and tracing spaces without leaving governance.

The OpenFeature specification and reference docs name a small public
`EvaluationDetails` surface for detailed flag evaluation calls. That surface is
already meant to answer a useful operational question:

> what value resolved, and why?

That is close to Assay's evidence discipline, but not because feature flags are
AI-specific. It is useful because the returned decision detail is:

- named
- small
- policy / release-control adjacent
- explicitly separate from provider configuration
- already public in a cross-provider standard

P30 should test whether Assay can import one policy-decision signal without
turning into a feature flag platform, observability backend, or app config
truth engine.

## 2. What this plan is and is not

This plan is for:

- one detailed flag evaluation result
- one `EvaluationDetails`-shaped returned object
- one bounded reduction of the fields naturally present on that object
- one discovery pass over a public OpenFeature SDK call
- one audit/debug-oriented upstream contribution if the docs reveal a real
  clarity gap

This plan is not for:

- full OpenFeature SDK support
- provider implementation support
- flag configuration import
- targeting rule import
- rollout, segment, or experiment truth
- hook execution truth
- OpenTelemetry flag evaluation telemetry
- app behavior or feature correctness truth
- provider metadata as a first-class Assay surface

## 3. Hard positioning rule

P30 v1 claims only one bounded OpenFeature `EvaluationDetails` object as
external decision-detail evidence. It does not claim the flag value is correct,
the provider is correctly configured, targeting rules are correct, rollout
state is correct, or the application made the right product decision.

That means:

- OpenFeature remains the public API context
- the provider remains the source of the observed detail fields
- Assay imports only a reduced decision-detail artifact
- Assay does not import provider state, targeting logic, or flag metadata as
  truth

## 4. Recommended surface

The first P30 surface should stay on exactly one move:

- call one public OpenFeature detailed evaluation method such as
  `getBooleanDetails` / `getStringDetails` / equivalent in one SDK
- capture the raw returned `EvaluationDetails` object separately
- reduce exactly one returned detail object

Not:

- basic value-only evaluation calls
- provider config
- flag definitions
- targeting rules
- hooks
- transaction context
- telemetry spans
- OFREP response envelopes
- bulk flag state
- vendor-specific dashboards or audit logs

The first sample should prefer a boolean flag detail because it is the smallest
honest decision shape. Wider typed values can be considered after discovery,
but v1 does not need object-valued flag evidence to prove the lane.

## 5. Canonical v1 artifact thesis

The v1 artifact should be frozen from a captured returned `EvaluationDetails`
object, not from a general reading of the OpenFeature spec or provider-side
resolution internals.

Illustrative v1 shape:

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
    "reason": "TARGETING_MATCH"
  }
}
```

Optional reviewer support, only if naturally present on the returned detail
object and small enough to preserve without provider drift:

- `result.error_code`
- `result.error_message`
- `flag_metadata_ref`

Not allowed in v1:

- provider configuration
- targeting rules
- segment definitions
- rollout percentages
- flag definition JSON
- hook state
- transaction context
- OpenTelemetry spans or metrics
- inline flag metadata bags by default
- synthetic user, request, or provider identifiers
- application feature correctness claims

## 6. Field boundaries

### 6.1 `flag_key`

`flag_key` is the natural evaluated-target anchor because OpenFeature detailed
evaluation fields include the flag key. It names what was evaluated.

It must not become:

- flag configuration truth
- rollout truth
- provider identity truth
- application feature identity beyond the evaluated key

### 6.2 `result.value`

`result.value` is the value returned by the detailed evaluation call.

For the first sample, use a boolean value unless discovery proves another
primitive value is the cleaner first surface. V1 should not start with
object-valued flags.

It must not be read as:

- the correct product decision
- the real runtime behavior of the app
- provider correctness
- user eligibility truth

### 6.3 `result.variant`

`variant` is optional and should be included only when naturally present.

It is useful for review because OpenFeature treats variant as the semantic name
of the resolved value when available.

It must not become:

- rollout bucket truth
- experiment assignment truth
- a stable user segment

### 6.4 `result.reason`

`reason` is optional and should remain short.

It names why the provider reported the value, not whether the evaluation was
business-correct. Do not widen it into a trace, hook log, or targeting-rule
explanation.

### 6.5 `result.error_code` / `result.error_message`

Error fields may be included for failure fixtures if naturally present.

They must name evaluation failure only. They must not become provider health,
incident, or application failure truth.

### 6.6 `flag_metadata_ref`

Provider / flag metadata is default out of scope for P30.

The reducer may include a bounded `flag_metadata_ref` only if discovery shows
the returned detail object naturally carries small metadata and there is a real
review need. Inline metadata bags are malformed for v1.

## 7. Observed vs derived rule

Capture separately:

- the raw returned `EvaluationDetails`
- the SDK language and package version
- the emitted call inputs needed to reproduce discovery

The canonical v1 artifact must not include:

- evaluation context
- targeting keys
- user identifiers
- provider config
- default value unless it naturally appears on the returned detail object
- synthetic hashes of omitted raw inputs

## 8. Cardinality rule

V1 is single-detail only.

Malformed for v1:

- arrays of flag details
- bulk flag-state payloads
- provider result maps
- OFREP envelopes
- telemetry batches
- partial import of the first item from a larger response without an explicit
  extracted single-detail discovery artifact

## 9. Discovery gate

Do not freeze fixtures until discovery captures:

- one valid returned `EvaluationDetails` object from a public SDK detailed
  evaluation call
- one abnormal or fallback case if it can be produced locally without external
  provider infrastructure
- raw emitted inputs stored separately from raw returned detail
- an explicit note on whether `variant`, `reason`, error fields, and flag
  metadata were naturally present
- an explicit language / SDK note

If SDKs differ materially in returned field names or value encoding, freeze P30
per SDK first. Do not pretend there is one cross-language artifact contract
until capture proves it.

## 10. Upstream contribution posture

P30 should not open with an abstract API-stability question.

The useful upstream move is audit/debug docs clarity:

> While checking the detailed evaluation docs, I noticed this part of the
> returned detail object was harder to inspect than the rest. This patch makes
> it clearer what users get back when they need to understand why a flag
> resolved a certain way.

Only contribute upstream if there is a concrete docs or example gap around:

- detailed evaluation methods
- the `EvaluationDetails` field table
- error detail examples
- flag metadata boundaries
- SDK-specific naming differences

Do not mention Assay unless maintainers ask what prompted the clarification.

## 11. Concrete repo deliverable

After this plan, the implementation PR should add:

- `examples/openfeature-evaluation-details-evidence/README.md`
- a small local capture probe using one public OpenFeature SDK
- raw discovery artifacts for emitted input and returned detail
- one reduced valid fixture
- one reduced fallback/error fixture if reproducible
- one malformed fixture for wider provider/bulk envelopes
- a mapper into the existing placeholder NDJSON pattern
- an `examples/README.md` index entry

## 12. Non-goals

P30 does not:

- implement OpenFeature provider support
- validate flag correctness
- import provider configuration
- import targeting rules
- import flag metadata inline by default
- model experiments or rollouts
- model app behavior after flag evaluation
- model OpenFeature telemetry

## References

- OpenFeature Specification — Flag Evaluation:
  https://openfeature.dev/specification/sections/flag-evaluation/
- OpenFeature Docs — Evaluation API:
  https://openfeature.dev/docs/reference/concepts/evaluation-api/
- OpenFeature Specification — Observability Appendix:
  https://openfeature.dev/specification/appendix-d
