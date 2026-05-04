# Receipt Families

Assay compiles selected external outcomes into portable evidence receipts and
bounded Trust Basis claims.

The current public claim-visible families are:

| Source surface | Receipt family | Trust Basis claim |
|---|---|---|
| Promptfoo assertion component result | eval outcome receipt | `external_eval_receipt_boundary_visible` |
| OpenFeature boolean `EvaluationDetails` | runtime decision receipt | `external_decision_receipt_boundary_visible` |
| CycloneDX ML-BOM `machine-learning-model` component | inventory / provenance receipt | `external_inventory_receipt_boundary_visible` |

Mastra score events and Pydantic case results are importer-only receipt lanes in
this line. They do not expose public Trust Basis claims.

Use the machine-readable [receipt family matrix](receipt-family-matrix.json) for
the source-of-truth family paths, schema names, included fields, excluded
fields, non-claims, and importer-only status.

Use the [receipt schema registry](receipt-schemas/README.md) for the bounded
JSON Schema contracts exposed through `assay evidence schema`.

For artifact-first proof, see [Evidence Receipts in Action](../notes/EVIDENCE-RECEIPTS-IN-ACTION.md).
