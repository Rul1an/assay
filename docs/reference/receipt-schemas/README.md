# Receipt Schema Registry

This directory records machine-readable JSON Schema contracts for the bounded
external receipt lanes that Assay imports.

These schemas are intentionally narrower than the upstream ecosystems they
reference. They describe the Assay-supported receipt or reduced input surface,
not full Promptfoo, OpenFeature, CycloneDX, Mastra, or Pydantic schemas.

## Directories

- `receipts/`: payload schemas for Assay receipt events stored in evidence
  bundles.
- `inputs/`: supported importer input artifact schemas where the import shape
  differs from the receipt payload.

## CLI

The registry is also available through the Assay CLI:

```bash
assay evidence schema list
assay evidence schema show promptfoo.assertion-component.v1
assay evidence schema validate --schema promptfoo.assertion-component.v1 --input receipt.json
```

Use `--jsonl` with `validate` for JSONL importer inputs.

## Boundary

The schemas do not make integration, endorsement, correctness, safety,
compliance, or full-platform truth claims. They only make the selected receipt
boundary inspectable and testable.

Schema `$id` values use release-tagged `raw.githubusercontent.com` URLs. They
point at the released `v3.8.0` contract line; later compatible releases carry
the same bounded receipt/import schemas forward unless release notes say
otherwise.
