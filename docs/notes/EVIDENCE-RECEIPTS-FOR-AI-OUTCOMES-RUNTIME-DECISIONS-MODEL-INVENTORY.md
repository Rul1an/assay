# Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory

> **Status:** public technical note
> **Last updated:** 2026-04-29
> **Scope:** explains the released Assay v3.9.1 and Assay Harness v0.3.2
> three-family receipt surface; adds no new Trust Basis claim, receipt family,
> or Harness semantics.

We have been tightening a downstream evidence pattern in Assay around a small
idea that keeps paying rent:

selected external system outputs can be reduced into small, verifiable evidence
receipts, and those receipts can feed bounded Trust Basis claims.

The point is not to make Assay the source of truth for Promptfoo, OpenFeature,
CycloneDX, or any other upstream system. The point is cleaner and more useful:
preserve a narrow, inspectable boundary around what was imported, what was
excluded, which reducer was used, and which Trust Basis claim became visible.

That boundary is the product. Boring in the right way: small, reviewable, and
predictable.

For the same idea as a small artifact-first proof, see
[Evidence Receipts in Action](EVIDENCE-RECEIPTS-IN-ACTION.md).

## Three Receipt Families

The current released line includes three receipt families. This note describes
the currently released receipt families only; it is not a promise that every
external surface will be modeled this way.

| Source surface | Assay receipt family | Trust Basis claim |
|---|---|---|
| Selected Promptfoo assertion component results | eval outcome receipts | `external_eval_receipt_boundary_visible` |
| Boolean OpenFeature `EvaluationDetails` outcomes | runtime decision receipts | `external_decision_receipt_boundary_visible` |
| Selected CycloneDX `machine-learning-model` components | inventory / provenance receipts | `external_inventory_receipt_boundary_visible` |

Each family is intentionally narrow.

Assay does not claim that an eval result is correct, that a feature-flag
decision was good, or that an ML-BOM proves model safety or provenance. It only
claims that a supported bounded receipt was present and that its boundary was
visible in the generated Trust Basis artifact.

## Why Receipts Instead Of Broad Integrations?

Broad integrations tend to blur responsibility. They are useful when you need
product depth, but they can be a bit much when the real job is evidence review.

For this line, we kept the contract smaller.

Promptfoo remains the eval runner. OpenFeature remains the feature-flag
evaluation surface. CycloneDX remains the inventory / BOM format. Assay
compiles selected downstream artifacts into evidence receipts. Assay Harness
gates and reports Trust Basis diffs without learning family-specific semantics.

In v1, receipts deliberately exclude raw prompts, raw outputs, targeting
context, provider internals, full BOM bodies, traces, scorer configs, and other
broad upstream payloads.

That split keeps the whole thing reviewable. Nobody has to pretend a small
downstream evidence compiler suddenly understands the full semantics of every
upstream ecosystem. We should not make it more beautiful than it is. The value
is that the boundary stays crisp.

## Released Surfaces

This note refers to versioned releases, not floating main branches:

- Assay `v3.9.1`: <https://github.com/Rul1an/assay/releases/tag/v3.9.1>
- Assay Harness `v0.3.2`: <https://github.com/Rul1an/Assay-Harness/releases/tag/v0.3.2>
- Receipt family matrix: <https://github.com/Rul1an/assay/blob/v3.9.1/docs/reference/receipt-family-matrix.json>
- Receipt schema registry: <https://github.com/Rul1an/assay/tree/v3.9.1/docs/reference/receipt-schemas>
- Harness compatibility note: <https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/ASSAY_COMPATIBILITY.md>

There is also an example release-binary proof run showing the Harness recipes
passing against Assay `v3.9.0`:

<https://github.com/Rul1an/Assay-Harness/actions/runs/25131209377>

Assay `v3.9.1` does not change receipt runtime behavior, Trust Basis claims,
or Harness semantics; it only publishes this note under an immutable Assay
release tag.

## Copyable Recipe Docs

The Harness recipes are downstream examples, not official integrations:

- Promptfoo receipt pipeline: <https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/PROMPTFOO_RECEIPT_PIPELINE.md>
- OpenFeature decision receipt pipeline: <https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/OPENFEATURE_DECISION_RECEIPT_PIPELINE.md>
- CycloneDX ML-BOM model receipt pipeline: <https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/CYCLONEDX_MLBOM_MODEL_RECEIPT_PIPELINE.md>

## Boundary

This is not an official Promptfoo, OpenFeature, or CycloneDX integration.

It is a downstream evidence-consumer pattern over public artifact surfaces. The
receipts are meant to be small, deterministic, and reviewable. They are not a
replacement for upstream tools, upstream semantics, or upstream governance
models.

The useful shape is simple:

```text
external system output
  -> bounded receipt
  -> verifiable bundle
  -> Trust Basis claim boundary
  -> Harness gate/report projection
```

That gives CI and reviewers something concrete to inspect without pretending
that Assay owns the truth of the original system.
