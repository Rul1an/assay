# PLAN — P43 CycloneDX ML-BOM Model Component Receipt Import

- **Date:** 2026-04-28
- **Owner:** Assay maintainers
- **Status:** execution slice
- **Scope:** Turn one selected CycloneDX ML-BOM `machine-learning-model`
  component into one portable Assay inventory receipt, without importing full
  BOM, model-card, dataset, graph, vulnerability, license, or compliance truth.
- **Target repo:** `Rul1an/assay`
- **Depends on:** P31-P34, P41

## One-line goal

Turn one selected CycloneDX ML-BOM `machine-learning-model` component into one
portable Assay inventory receipt.

## 1. Why this slice exists

P31 proved eval outcome receipts with Promptfoo. P41 proved runtime decision
receipts with OpenFeature. P43 opens the third family: inventory and
provenance-adjacent receipts.

CycloneDX ML-BOM is a strong next seam because it already names AI/ML inventory
surfaces: models, datasets, dependencies, model cards, and provenance. That
surface is intentionally rich. P43 deliberately does not become a BOM viewer or
compliance layer. It imports one model-component boundary and preserves a digest
link to the richer source BOM.

This is not CycloneDX support in the broad sense. It is an Assay-side compiler
lane over one bounded model-component inventory surface.

## 2. Layering

The stack boundary is:

```text
CycloneDX ML-BOM JSON
  -> selected components[] machine-learning-model entry
  -> assay evidence import cyclonedx-mlbom-model
  -> Assay EvidenceEvent receipt bundle
  -> evidence verify / trust-basis generate
```

Assay owns receipt reduction and bundle semantics. CycloneDX remains the BOM
standard and source artifact context, not the Assay evidence contract.

Harness is intentionally out of scope for P43. Harness may later provide a
recipe over Trust Basis artifacts, but it must not parse CycloneDX, inspect
receipt payloads, or define inventory drift semantics.

## 3. Scope

P43 v1 imports exactly one bounded path:

- one CycloneDX JSON BOM
- one selected `components[]` entry
- `component.type = "machine-learning-model"`
- one Assay EvidenceEvent receipt

If the BOM has exactly one model component, the importer may select it. If the
BOM has multiple model components, `--bom-ref` is required. If `--bom-ref` does
not match a model component, the importer fails closed.

## 4. Input surface

P43 consumes CycloneDX JSON BOM files where the selected model is a component:

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.7",
  "components": [
    {
      "bom-ref": "pkg:huggingface/example/model@abc123",
      "type": "machine-learning-model",
      "publisher": "Example Inc.",
      "name": "example-model",
      "version": "1.0.0",
      "purl": "pkg:huggingface/example/model@abc123",
      "modelCard": {
        "bom-ref": "model-card-example-model",
        "modelParameters": {
          "datasets": [{ "ref": "component-training-data" }]
        }
      }
    }
  ]
}
```

The importer does not claim that all CycloneDX ML-BOM authoring patterns are in
scope. In particular, P43 v1 does not import `metadata.component` subject BOMs
or expand BOM graphs. Those may become separate lanes if there is a clear small
surface.

## 5. Receipt v1 thesis

The receipt body is an Assay EvidenceEvent payload:

```json
{
  "schema": "assay.receipt.cyclonedx.mlbom-model-component.v1",
  "source_system": "cyclonedx",
  "source_surface": "bom.components[type=machine-learning-model]",
  "source_artifact_ref": "bom.cdx.json",
  "source_artifact_digest": "sha256:...",
  "reducer_version": "assay-cyclonedx-mlbom-model-component@0.1.0",
  "imported_at": "2026-04-28T12:00:00Z",
  "model_component": {
    "bom_ref": "pkg:huggingface/example/model@abc123",
    "name": "example-model",
    "version": "1.0.0",
    "publisher": "Example Inc.",
    "purl": "pkg:huggingface/example/model@abc123",
    "dataset_refs": ["component-training-data"],
    "model_card_refs": ["model-card-example-model"]
  }
}
```

Optional fields are omitted when absent or null. The receipt remains an
inventory receipt. It does not become a model card, dataset card, full BOM, or
AI governance declaration.

## 6. Field rules

`model_component.bom_ref` is required. It identifies the selected component
inside the BOM only. It is not proof that the referenced package, model, or
repository exists.

`model_component.name` is required. It is the component name carried by the BOM.

`model_component.version`, `publisher`, and `purl` are optional bounded strings
when naturally present on the selected component. The importer must not invent
or resolve identity from external systems.

`model_component.dataset_refs` are optional bounded refs taken from
`modelCard.modelParameters.datasets[].ref`. They are refs only. The importer
does not expand dataset components or import dataset bodies.

`model_component.model_card_refs` are optional bounded refs taken from
`modelCard.bom-ref`. They are refs only. The importer does not import the
`modelCard` body.

## 7. Strict exclusions

P43 v1 excludes:

- full BOM contents
- `metadata.component` subject import
- dependency graphs, compositions, and BOM-Link traversal
- vulnerabilities, VEX, licenses, and legal conclusions
- pedigree, ancestors, and lineage graph expansion
- full `modelCard` bodies
- dataset component bodies and dataset contents
- performance metrics and quantitative analysis
- ethical, fairness, limitation, and environmental sections
- network dereferencing or URL resolution
- compliance, approval, safety, security, or model-correctness claims

Those fields may be important in a CycloneDX ML-BOM. P43 leaves them in the
source artifact and binds to that artifact by digest.

## 8. Trust Basis posture

P43 does not add an inventory-specific Trust Basis claim.

The first slice proves that CycloneDX model-component receipts are:

- bundleable
- verifiable
- source-digest-bound
- readable by the Trust Basis path

A later slice may add a claim such as
`external_inventory_receipt_boundary_visible`, but only after the receipt
contract has landed and the claim semantics are narrow enough to avoid
compliance or model-truth overclaiming.

## 9. Acceptance criteria

- `assay evidence import cyclonedx-mlbom-model` writes a verifiable bundle.
- Exactly one selected model component becomes exactly one receipt event.
- Multiple model components without `--bom-ref` fail closed.
- A missing or non-model `--bom-ref` fails closed.
- Full `modelCard`, dataset body, vulnerability, license, and pedigree content
  do not appear in the receipt payload.
- The receipt type is registered as experimental in Evidence Contract v1.
- CLI docs and an example fixture explain the boundary.

## 10. Follow-ups

P44 should live in Assay Harness as a recipe over existing contracts:

```text
CycloneDX ML-BOM -> Assay model receipt bundle -> Trust Basis -> diff -> Harness gate/report
```

P44 must not define model-version or dataset-ref drift by parsing receipts in
Harness. If inventory drift becomes product-critical, Assay should first define
an inventory diff or inventory Trust Basis claim contract.
