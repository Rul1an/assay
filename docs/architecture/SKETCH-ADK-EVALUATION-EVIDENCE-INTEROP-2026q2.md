# Sketch: Google ADK Evaluation Evidence Interop

Date: 2026-04-06
Status: v1 sketch only

## Purpose

This note sketches the smallest useful interop sample between Assay and
Google ADK.

It is intentionally narrow. It is not a roadmap commitment, a partnership
announcement, or a request for ADK to grow a new governance feature.

The goal is simpler:

- let ADK keep doing agent building, evaluation, and deployment
- let Assay keep compiling bounded external evidence
- test one honest handoff between them

## Current read

ADK looks strongest where it is already explicit:

- building, evaluating, and deploying agents
- trajectory-aware evaluation
- artifacts as portable stored output
- CI-friendly evaluation workflows

Assay looks strongest when it turns upstream outputs into portable,
reviewable evidence:

- deterministic evidence bundles
- Trust Basis
- Trust Card
- CI-facing outputs such as SARIF

The overlap is real, but the products do not need to become each other.

## Recommended v1 seam

Use **one exported ADK evaluation artifact** as the first interop surface.

More specifically:

- start with one small trajectory-aware evaluation result
- map only a bounded subset into Assay evidence
- treat the rest as out of scope for v1

This is a better first seam than broad runtime-governance language because ADK
already presents evaluation and artifacts as first-class product surfaces. The
question is not "can you add an audit layer for us?" It is "is there a
smallest stable evaluation output an external evidence consumer can safely
read?"

## See in the ADK docs

- `https://adk.dev/evaluate/`
- `https://adk.dev/artifacts/`

These docs make the v1 direction clear enough:

- ADK evaluates both final response quality and execution trajectory
- ADK treats artifacts as portable stored output with versioning and scope

## Example input

The v1 sample should use one tiny exported evaluation artifact. The exact field
names should follow whatever ADK already treats as stable for external
consumers.

Illustrative shape:

```json
{
  "eval_case_id": "travel_policy_001",
  "run_id": "run_42",
  "timestamp": "2026-04-06T10:14:23Z",
  "outcome": "pass",
  "trajectory": {
    "expected_steps": [
      "determine_intent",
      "use_tool",
      "review_results",
      "report_generation"
    ],
    "actual_steps": [
      "determine_intent",
      "use_tool",
      "review_results",
      "report_generation"
    ]
  },
  "metrics": {
    "tool_trajectory_score": 1.0,
    "response_match_score": 0.92
  }
}
```

This example is intentionally small. It is just enough to test whether Assay
can consume a bounded ADK evaluation output without pretending to own ADK's
evaluator semantics.

## Minimal Assay mapping

Assay should treat this as **external evaluation evidence**.

Suggested imported evidence event shape (ADR-006-style, abbreviated envelope):

```json
{
  "specversion": "1.0",
  "type": "external.evaluation.result",
  "source": "google:adk",
  "time": "2026-04-06T10:14:23Z",
  "data": {
    "eval_case_id": "travel_policy_001",
    "run_id": "run_42",
    "outcome": "pass",
    "expected_steps": [
      "determine_intent",
      "use_tool",
      "review_results",
      "report_generation"
    ],
    "actual_steps": [
      "determine_intent",
      "use_tool",
      "review_results",
      "report_generation"
    ],
    "tool_trajectory_score": 1.0,
    "response_match_score": 0.92
  }
}
```

The important thing is not the exact event shape. The important thing is that
Assay stays honest about what it observed.

## What stays observed

In v1, Assay should keep the imported ADK signal in the observed bucket:

- evaluation case identity
- run identity
- timestamps
- expected and actual step lists
- evaluation outcome
- evaluator scores and metrics

## What Assay should not import as truth

We are not asking to import ADK evaluator scores, trajectory judgments, or
deployment semantics into Assay as truth. We are asking whether there is a
smallest stable evaluation output that can be compiled into bounded external
evidence.

That means v1 should explicitly avoid:

- score translation into Assay trust language
- collapsing evaluator output into Assay trust claims
- implying that Assay independently verified evaluation correctness
- implying that a passing ADK evaluation means the system is safe

## Why this helps

- it gives Assay a real evaluation corpus instead of toy examples
- it gives ADK a portable review path for evaluation results outside the ADK
  runtime itself
- it keeps the interop seam small enough to discuss without turning into a
  platform merger story

## External ask

If ADK maintainers engage, the best next ask is still small:

- point to one smallest stable evaluation artifact or trajectory output for
  external consumers
- provide one tiny sample artifact
- confirm which fields are intentionally stable enough to consume

That is enough to decide whether a real interop sample is worth building.
