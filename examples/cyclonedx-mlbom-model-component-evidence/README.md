# CycloneDX ML-BOM Model Component Evidence

This example shows the P43 inventory receipt lane:

```text
CycloneDX ML-BOM JSON
  -> assay evidence import cyclonedx-mlbom-model
  -> Assay model-component inventory receipt bundle
  -> assay evidence verify
  -> assay trust-basis generate
```

The example is intentionally small. It treats one selected CycloneDX
`machine-learning-model` component as a bounded inventory surface, not as full
BOM truth.

## Run

```bash
assay evidence import cyclonedx-mlbom-model \
  --input examples/cyclonedx-mlbom-model-component-evidence/fixtures/model.cdx.json \
  --bundle-out /tmp/cyclonedx-model-receipt.tar.gz \
  --source-artifact-ref model.cdx.json \
  --import-time 2026-04-28T12:00:00Z \
  --run-id cyclonedx_model_example

assay evidence verify /tmp/cyclonedx-model-receipt.tar.gz
assay trust-basis generate /tmp/cyclonedx-model-receipt.tar.gz
```

## Boundary

P43 v1 imports one `components[]` entry with
`type = "machine-learning-model"`.

If the BOM has exactly one model component, the importer can select it. If the
BOM has more than one model component, pass `--bom-ref` so the receipt does not
silently guess which model matters.

The receipt may include compact model identity fields plus bounded refs:

- `bom_ref`
- `name`
- optional `version`
- optional `publisher`
- optional `purl`
- optional `dataset_refs`
- optional `model_card_refs`

The importer does not dereference refs, resolve network URLs, expand dataset
components, or import full `modelCard` bodies.

## Formulation Fixture

`fixtures/model-handoff-formulation.cdx.json` extends the small model example
with CycloneDX `formulation[]` workflow data. The formulation shows an
illustrative model training, evaluation, and evidence-handoff workflow using
CycloneDX workflow, task, step, metric-output, and evidence-output structures.

For this importer, formulation remains source-BOM context. Assay still reduces
only the selected `machine-learning-model` component plus bounded refs, and the
receipt binds back to the exact CycloneDX artifact through
`source_artifact_digest`. Formulation, metrics, task outputs, and evidence
external references are not promoted into evaluation or decision receipts.

Formulation parsing is intentionally out of scope for this importer. It may be
considered later only for a separate formulation or evaluation receipt lane
backed by a real downstream consumer request. Until then, formulation stays
bound through the source BOM digest.

The handoff task in this fixture uses an Assay command because this is the
Assay-side consumer example. A generic CycloneDX example should keep the same
structure but replace that command with a project-neutral evidence producer.

## Not Imported

The v1 receipt excludes:

- full BOM contents
- dependency graphs and composition
- vulnerabilities and licenses
- pedigree and ancestors
- full `modelCard` bodies
- dataset component bodies
- model metrics
- ethical, fairness, and limitation sections
- formulation workflows, tasks, steps, metrics, and handoff outputs

Those fields are important in CycloneDX ML-BOMs. P43 simply keeps the Assay
receipt unit small enough to review and portable enough to bundle.
