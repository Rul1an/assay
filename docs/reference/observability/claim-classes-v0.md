# Observability Claim Classes v0

> **Status:** research/reference contract for the
> observability-layering line. This document defines vocabulary for
> comparison rows; it is not a Runner archive artifact, not a Trust
> Basis claim, and not a product-facing compliance surface.

Claim classes answer one narrow question:

```text
Given an observability artifact, what kind of claim can it honestly
support, and on what basis?
```

The contract exists so traces, measured-run archives, and joined
artifacts can be compared without turning every observation into the
same kind of evidence.

## Schema String

```text
assay.observability.claim_class_cell.v0
```

Machine-readable schema:

[`schema/claim-class-cell-v0.schema.json`](schema/claim-class-cell-v0.schema.json)

## Vocabulary

Each claim cell uses two axes:

```json
{
  "claim_strength": ["strong", "partial", "weak", "absent"],
  "claim_basis": ["reported", "measured", "derived", "inferred"]
}
```

### Claim Strength

| Value | Meaning |
|---|---|
| `strong` | The artifact directly supports the claim inside its declared boundary. |
| `partial` | The artifact supports part of the claim, but another layer or assumption is needed. |
| `weak` | The artifact provides context or a hint, but not enough for a reviewable claim. |
| `absent` | The artifact does not support the claim. |

### Claim Basis

| Value | Meaning |
|---|---|
| `reported` | The claim comes from an SDK, framework, trace, app hook, or other self-reported source. |
| `measured` | The claim comes from a measured runtime source such as cgroup-scoped kernel events or Runner observation health. |
| `derived` | The claim is computed from explicit source artifacts by a declared rule. |
| `inferred` | The claim depends on interpretation that is not directly carried by the source artifacts. |

`inferred` is allowed so weak comparison rows can be explicit, but it
should not carry the main result of a findings document. Prefer moving
inferred statements into threats-to-validity text unless the inference
rule is itself the subject of the experiment.

## Cell Shape

A claim cell records one artifact's support for one claim type:

```json
{
  "schema": "assay.observability.claim_class_cell.v0",
  "claim_type": "measured_filesystem_effect",
  "artifact_role": "measured_run_archive",
  "claim_strength": "strong",
  "claim_basis": "measured",
  "evidence_refs": [
    "observation-health.json",
    "capability-surface.json"
  ],
  "notes": [
    "Only valid when observation health is clean."
  ],
  "non_claims": [
    "does_not_prove_tool_intent"
  ]
}
```

## Artifact Roles

| Role | Meaning |
|---|---|
| `otel_family_trace` | An OpenTelemetry-family trace, including OpenInference-style semantic conventions. |
| `measured_run_archive` | An Assay-Runner measured-run archive or extracted archive contents. |
| `joined_artifacts` | A comparison row that uses both trace and measured-run evidence through an explicit join key. |
| `external_receipt` | A receipt or verifier output imported as external evidence. |
| `none` | No artifact supports the claim. |

## Contract Principles

1. **Strength and basis are independent.** A claim can be `strong` but
   `reported`, or `partial` but `measured`.
2. **Strong does not mean universal.** A strong claim is strong only
   inside the artifact's declared boundary.
3. **Measured does not mean semantic.** Kernel or capability-surface
   evidence can prove an effect occurred without proving why it occurred.
4. **Reported does not mean false.** Reported trace or SDK fields can be
   the right source for intent and control flow.
5. **Derived must name the rule.** A derived claim should include the
   comparator, projection, or schema rule that produced it in
   `evidence_refs` or `notes`.
6. **Absent is a claim about the artifact, not the system.** `absent`
   means the artifact does not support the claim; it does not prove the
   underlying event did not happen.

## Canonical Claim Types For The Layering Experiment

The first observability-layering findings document should use this
starter set unless it explicitly freezes a new version.

The v0 JSON schema keeps `claim_type` as an open lowercase identifier
rather than an enum. That lets the first findings document add a small
experiment-specific row without a schema bump. The tradeoff is typo
risk, so findings should treat the table below as canonical unless they
also document a new claim type. A v0.1 contract may freeze this starter
set after the first findings document proves it is stable enough.

| Claim type | Description |
|---|---|
| `reported_control_flow` | The agent/framework-reported execution shape. |
| `tool_call_intent_context` | Tool target, declared arguments or projections, and surrounding semantic context. |
| `tool_call_identity` | Stable identity for joining tool-call records across layers. |
| `policy_decision_evidence` | The allow/deny/error decision and policy context observed for a tool call. |
| `measured_filesystem_effect` | Filesystem paths or operations observed inside the measurement boundary. |
| `measured_network_effect` | Network endpoints observed inside the measurement boundary. |
| `process_execution_effect` | Process execution observed inside the measurement boundary. |
| `bounded_negative_claim` | A negative claim scoped to clean measurement health. |
| `measurement_integrity` | Health, drop, and correlation signals that bound measured claims. |
| `capability_drift` | Cross-run or cross-arm difference in observed capability surface. |
| `privacy_capture_policy` | What content the artifact may expose or deliberately omit by configuration/design. |

## Non-Claims

- This contract does not rank observability products.
- This contract does not decide whether a policy decision is correct.
- This contract does not make legal or compliance claims.
- This contract does not promote comparison rows into Trust Basis claims.
- This contract does not replace Runner archive health gates.
