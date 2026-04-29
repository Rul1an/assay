# Receipt Schema Registry

This directory records machine-readable JSON Schema contracts for the bounded
external receipt lanes that Assay imports.

These schemas are intentionally narrower than the upstream ecosystems they
reference. They describe the Assay-supported receipt or reduced input surface,
not full Promptfoo, OpenFeature, CycloneDX, or Mastra schemas.

## Directories

- `receipts/`: payload schemas for Assay receipt events stored in evidence
  bundles.
- `inputs/`: supported importer input artifact schemas where the import shape
  differs from the receipt payload.

## Boundary

The schemas do not make integration, endorsement, correctness, safety,
compliance, or full-platform truth claims. They only make the selected receipt
boundary inspectable and testable.
