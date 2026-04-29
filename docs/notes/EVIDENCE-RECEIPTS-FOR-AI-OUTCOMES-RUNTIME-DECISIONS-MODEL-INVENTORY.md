# Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory

> **Status:** technical note
> **Last updated:** 2026-04-29
> **Scope:** explains the Assay v3.8.0 receipt-family and schema surface; adds no new Trust Basis claim or Harness semantics

Modern AI governance stacks do not only need more tests, traces, or dashboards.
They need small, portable evidence units at the existing seams where important
things already happen.

Assay v3.7.0 made the first three of those seams claim-visible, and v3.8.0
adds machine-readable schema contracts for the supported receipt/import
surfaces:

- Promptfoo assertion component results become eval outcome receipts.
- OpenFeature boolean `EvaluationDetails` become runtime decision receipts.
- CycloneDX ML-BOM model components become inventory/provenance receipts.

Those are not integration claims. They are compiler lanes over selected,
bounded surfaces that already exist in upstream systems.

## The Thesis

AI systems produce many useful artifacts: eval outputs, runtime decisions,
model inventories, traces, logs, dashboards, scorecards, and reports.

Most of those artifacts are too large, too contextual, or too
platform-specific to use directly as portable review evidence. The useful seam
is smaller:

```text
one selected outcome or decision or inventory surface
  -> one bounded receipt
  -> one verifiable bundle
  -> one claim-level Trust Basis artifact
```

Assay is the evidence compiler for that smaller seam.

## The Three Families

### Eval Outcome Receipts

Promptfoo can make AI behavior testable in CI. Assay imports one supported
assertion component result as a bounded receipt.

Trust Basis may then say:

- `external_eval_receipt_boundary_visible`

That claim means the supported receipt boundary and provenance are visible. It
does not mean the Promptfoo run passed, the model output was correct, or the
full Promptfoo export is Assay truth.

The deeper note is:

- [From Promptfoo JSONL to Evidence Receipts](FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md)

### Runtime Decision Receipts

OpenFeature can surface application-facing flag evaluation details. Assay
imports one supported boolean `EvaluationDetails` row as a bounded decision
receipt.

Trust Basis may then say:

- `external_decision_receipt_boundary_visible`

That claim means the supported decision receipt boundary and provenance are
visible. It does not mean the flag decision was correct, the provider behaved
correctly, targeting rules are true, or application behavior is safe.

### Inventory / Provenance Receipts

CycloneDX ML-BOM can describe AI/ML inventory surfaces. Assay imports one
selected `machine-learning-model` component as a bounded inventory receipt.

Trust Basis may then say:

- `external_inventory_receipt_boundary_visible`

That claim means the supported inventory receipt boundary and provenance are
visible. It does not mean the BOM is complete, the model is safe, the model
card is correct, dataset refs are approved, or the CycloneDX artifact is Assay
truth.

## The Shared Discipline

The three lanes are deliberately narrow.

Each lane includes:

- the source system and source surface,
- a reviewer-safe source artifact reference,
- a source artifact digest,
- a reducer version,
- an import timestamp,
- and a small domain-specific payload.

Each lane excludes raw bodies and broader platform truth. Promptfoo raw prompts
and outputs stay out. OpenFeature context, targeting keys, provider metadata,
flag metadata, and `error_message` stay out. CycloneDX full BOM graphs, model
card bodies, dataset bodies, vulnerabilities, licenses, and compliance posture
stay out.

That is the point. The receipt is not a mini version of the upstream platform.
It is a portable evidence unit derived from a selected seam.

## Canonical Artifacts

The canonical artifacts are:

- the Assay evidence bundle,
- the Trust Basis JSON,
- and the raw `assay.trust-basis.diff.v1` JSON when comparing runs.

Markdown, JUnit, job summaries, and recipe output are projections. They help
humans review the result, but they are not the source of truth.

The machine-readable family matrix is:

- [Receipt family matrix](../reference/receipt-family-matrix.json)
- [Receipt schema registry](../reference/receipt-schemas/README.md)

## Harness Boundary

Assay Harness v0.3.1 proves the same generic gate/report layer can carry all
three families.

Harness does not parse Promptfoo JSONL, OpenFeature JSONL, CycloneDX BOMs, or
Assay receipt payloads. It does not compare assertion values, flag decisions,
model versions, dataset refs, or domain-specific metadata. It preserves,
gates, and projects the raw Assay Trust Basis diff contract.

Runnable recipes live in Assay Harness:

- [Promptfoo receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.1/docs/PROMPTFOO_RECEIPT_PIPELINE.md)
- [OpenFeature decision receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.1/docs/OPENFEATURE_DECISION_RECEIPT_PIPELINE.md)
- [CycloneDX ML-BOM model receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.1/docs/CYCLONEDX_MLBOM_MODEL_RECEIPT_PIPELINE.md)

## What This Does Not Claim

This note does not claim official integration, partnership, endorsement, or
support from Promptfoo, OpenFeature, CycloneDX, or any runtime provider.

It also does not claim compliance, safety, correctness, model quality, dataset
approval, or complete inventory truth.

The claim is smaller and more useful:

```text
selected upstream seam -> bounded receipt -> portable review evidence
```

That is enough to make eval outcomes, runtime decisions, and inventory surfaces
reviewable without turning Assay into an eval runner, flag platform, BOM
viewer, or compliance dashboard.
