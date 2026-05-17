# CycloneDX ML-BOM Model to Inventory Receipt

Use this if your build already produces CycloneDX ML-BOMs and you want a small
reviewable CI artifact for the model inventory/provenance boundary that existed.

Assay does not replace CycloneDX or validate the full BOM. CycloneDX carries
the inventory artifact. Assay reduces one selected `machine-learning-model`
component into an inventory receipt, bundles it, verifies the bundle, and lets
CI gate the Trust Basis diff above that bundle.

## Problem

A CycloneDX ML-BOM can carry broad model inventory, model-card references,
dataset references, licenses, vulnerabilities, pedigree, and other supply-chain
metadata. That is valuable, but often too broad for a small CI review artifact.

The smaller review question is:

```text
Which selected model inventory/provenance boundary was observed, what source
artifact did it come from, and can that boundary be reviewed without importing
full BOM truth?
```

That is the receipt boundary. It is useful when reviewers need a portable
artifact showing that a supported model component boundary was present, without
pulling the whole BOM graph or treating the model as safe.

## One Workflow

First write a CycloneDX JSON BOM that contains a selected
`machine-learning-model` component.

Then import that selected model component into an Assay evidence bundle:

```bash
assay evidence import cyclonedx-mlbom-model \
  --input bom.cdx.json \
  --bundle-out evidence.tar.gz \
  --source-artifact-ref bom.cdx.json
```

Verify the bundle and compile the claim artifact:

```bash
assay evidence verify evidence.tar.gz
assay trust-basis generate evidence.tar.gz \
  --out cyclonedx-model.trust-basis.json
```

Compare the candidate Trust Basis against a baseline:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  cyclonedx-model.trust-basis.json \
  --format json \
  --fail-on-regression
```

In CI, the baseline Trust Basis artifact usually comes from the default branch
or a previously approved run.

Harness owns orchestration, exit codes, Markdown, and JUnit projection. The
released recipe is here:

- [CycloneDX ML-BOM model receipt pipeline](https://github.com/Rul1an/Assay-Harness/blob/v0.3.2/docs/CYCLONEDX_MLBOM_MODEL_RECEIPT_PIPELINE.md)

## Artifact Chain

```text
CycloneDX ML-BOM JSON
  -> assay evidence import cyclonedx-mlbom-model
  -> evidence.tar.gz
  -> assay trust-basis generate
  -> trust-basis.json
  -> assay trust-basis diff
  -> assay.trust-basis.diff.v1
  -> assay-harness trust-basis gate/report
```

## Canonical Artifact

The supported source boundary is one CycloneDX JSON BOM with a selected
`components[]` entry where `type` is `machine-learning-model`. Assay imports
bounded model-component fields and provenance references. It does not import
full BOM graphs or model-card bodies.

Tiny bounded source excerpt:

```json
{
  "$schema": "http://cyclonedx.org/schema/bom-1.7.schema.json",
  "bomFormat": "CycloneDX",
  "specVersion": "1.7",
  "components": [
    {
      "bom-ref": "pkg:huggingface/example/checkout-risk-model@def456",
      "type": "machine-learning-model",
      "publisher": "Example Inc.",
      "name": "checkout-risk-model",
      "version": "1.1.0",
      "purl": "pkg:huggingface/example/checkout-risk-model@def456",
      "modelCard": {
        "bom-ref": "model-card-checkout-risk-model-v1-1",
        "modelParameters": {
          "datasets": [
            {
              "ref": "component-checkout-risk-training-data-v2"
            }
          ]
        }
      }
    }
  ]
}
```

The current receipt lane is intentionally strict. It imports the selected model
component's bounded identity fields and reference fields: `bom-ref`, `name`,
and, when present, `version`, `publisher`, `purl`, model-card refs, and dataset
refs. Dataset bodies, model-card bodies, vulnerabilities, licenses, pedigree,
metrics, and fairness/ethics sections stay outside the receipt boundary.

Assay does not carry the `modelCard` body or dataset bodies forward. It reduces
only bounded identity and reference material from the selected component seam.

Proof artifacts are checked in under the Evidence Receipts in Action assets:

| Artifact | Role |
|---|---|
| [`candidate.cdx.json`](../assets/evidence-receipts-in-action/cyclonedx/candidate.cdx.json) | Tiny CycloneDX ML-BOM source artifact |
| [`evidence.tar.gz`](../assets/evidence-receipts-in-action/cyclonedx/evidence.tar.gz) | Verifiable Assay inventory receipt bundle |
| [`trust-basis.json`](../assets/evidence-receipts-in-action/cyclonedx/trust-basis.json) | Canonical claim artifact |
| [`trust-basis.diff.json`](../assets/evidence-receipts-in-action/cyclonedx/trust-basis.diff.json) | Canonical CI diff artifact |
| [`trust-basis-summary.md`](../assets/evidence-receipts-in-action/cyclonedx/trust-basis-summary.md) | Markdown reviewer projection |
| [`junit-trust-basis.xml`](../assets/evidence-receipts-in-action/cyclonedx/junit-trust-basis.xml) | JUnit CI projection |

## Inventory Receipt

The reduced receipt keeps the selected model boundary and source artifact
digest:

```json
{
  "schema": "assay.receipt.cyclonedx.mlbom-model-component.v1",
  "source_system": "cyclonedx",
  "source_surface": "bom.components[type=machine-learning-model]",
  "source_artifact_ref": "candidate.cdx.json",
  "source_artifact_digest": "sha256:6b0618708f49e3da21bda99a5dc82ce5409cbaa2e39d152b42fc90bc70f694ac",
  "reducer_version": "assay-cyclonedx-mlbom-model-component@0.1.0",
  "imported_at": "2026-04-28T10:01:00Z",
  "model_component": {
    "bom_ref": "pkg:huggingface/example/checkout-risk-model@def456",
    "name": "checkout-risk-model",
    "version": "1.1.0",
    "publisher": "Example Inc.",
    "purl": "pkg:huggingface/example/checkout-risk-model@def456",
    "model_card_refs": [
      "model-card-checkout-risk-model-v1-1"
    ],
    "dataset_refs": [
      "component-checkout-risk-training-data-v2"
    ]
  }
}
```

## Boundary

Assay may claim that a supported external inventory receipt boundary is visible:

```json
{
  "id": "external_inventory_receipt_boundary_visible",
  "level": "verified",
  "source": "external_inventory_receipt",
  "boundary": "supported-external-inventory-receipt-events-only"
}
```

That claim means one selected CycloneDX ML-BOM model component was reduced into
a supported receipt shape, carried through a verifiable bundle, and compiled
into a Trust Basis artifact.

When present, bounded identity fields such as `version`, `publisher`, and `purl`
may also remain visible in the reduced receipt.

It does not mean Assay owns CycloneDX semantics.

## Not Claimed

This path does not claim:

- the BOM is complete
- the model is safe, approved, or correct
- the model card is correct
- the referenced datasets are approved
- the licenses or vulnerabilities were evaluated
- CycloneDX officially supports Assay
- the full CycloneDX artifact is imported as Assay truth

The claim is about a reviewable inventory/provenance-reference boundary, not
model safety.

## Payoff Preview

The raw diff stays the canonical CI artifact:

```json
{
  "schema": "assay.trust-basis.diff.v1",
  "summary": {
    "regressed_claims": 0,
    "removed_claims": 0,
    "unchanged_claim_count": 10,
    "has_regressions": false
  }
}
```

In the non-regression path, this shows that the inventory receipt boundary
remained visible and unchanged.

The Markdown projection is intentionally smaller:

```text
Trust Basis Gate
Status: OK
Regressed claims: 0
Removed claims: 0
Unchanged claims: 10
```

Markdown and JUnit are review projections only. The raw JSON diff remains the
canonical CI artifact.

## When to Use This

Use this path when:

- a CycloneDX ML-BOM already exists in your build or release process
- reviewers need a portable artifact for a model inventory/provenance boundary
- you want Trust Basis diffs and Harness gates above selected model components
- model-card bodies, dataset bodies, licenses, vulnerabilities, and full BOM
  graphs should stay out of the receipt boundary

For the Assay CLI import reference, see
[`assay evidence import cyclonedx-mlbom-model`](../reference/cli/evidence.md#cyclonedx-ml-bom-model-import).
For the upstream CycloneDX seam, see the
[ML-BOM capability](https://cyclonedx.org/capabilities/mlbom/) and
[AI Models and Model Cards use case](https://cyclonedx.org/use-cases/ai-models-and-model-cards/).
For the three-family static proof page, see
[Evidence Receipts in Action](../notes/EVIDENCE-RECEIPTS-IN-ACTION.md).
