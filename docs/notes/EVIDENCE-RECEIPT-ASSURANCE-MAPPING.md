# Evidence Receipt Assurance Mapping

> **Status:** mapping note
> **Last updated:** 2026-05-03
> **Scope:** maps the released three-family receipt surface to common
> assurance questions. This note adds no new receipt family, Trust Basis claim,
> Harness behavior, compliance claim, or upstream integration claim.

Assay compiles selected external outcomes into portable evidence receipts and
bounded Trust Basis claims.

This note maps the released receipt families to the assurance questions they
can help answer. It does not turn Assay into a compliance oracle or make
upstream systems true. It only maps the current receipt boundaries to the
questions they can help answer.

## How to Read This

This is an assurance-oriented mapping note, not a compliance checklist or legal
interpretation.

| Column | Meaning |
|---|---|
| Assurance question | The kind of review question a team may be trying to answer. |
| Receipt family | The released Assay receipt family that can make the boundary visible. |
| What Assay makes visible | The bounded receipt boundary and Trust Basis claim Assay can compile from a verified bundle. |
| What Assay does not claim | The upstream, semantic, legal, or operational truth that remains outside Assay. |

The source of truth for paths, schemas, included fields, excluded fields, and
non-claims remains the [receipt family matrix](../reference/receipt-family-matrix.json).
For artifact proof, use [Evidence Receipts in Action](EVIDENCE-RECEIPTS-IN-ACTION.md).

## Mapping

| Assurance question | Receipt family | What Assay makes visible | What Assay does not claim |
|---|---|---|---|
| What was tested? | Promptfoo eval outcome receipts | A selected Promptfoo assertion component result was reduced into a bounded receipt, bundled, verified, and compiled into `external_eval_receipt_boundary_visible`. | The eval run passed, the model output was correct, Promptfoo is complete truth, or raw prompts/outputs/vars were imported. |
| What was decided at runtime? | OpenFeature runtime decision receipts | A boolean OpenFeature `EvaluationDetails` outcome was reduced into a bounded decision receipt, bundled, verified, and compiled into `external_decision_receipt_boundary_visible`. | The flag decision was correct, targeting rules were correct, provider behavior was safe, or evaluation context/config was imported. |
| What was the system built with? | CycloneDX ML-BOM inventory/provenance receipts | A selected CycloneDX `machine-learning-model` component was reduced into a bounded inventory receipt, bundled, verified, and compiled into `external_inventory_receipt_boundary_visible`. | The BOM is complete, the model is safe, licenses/vulnerabilities are covered, or full dependency/model-card/dataset truth was imported. |

## Family Detail

### Promptfoo

Promptfoo remains the eval runner. Assay only consumes selected assertion
component outcomes as receipt input.

Useful assurance framing:

- A reviewer can see that a supported eval outcome boundary exists in the
  evidence bundle.
- A reviewer can inspect the assertion type, pass/score fields, bounded reason
  when present, source artifact reference, source artifact digest, reducer
  version, and import timestamp.
- A CI gate can block if the Trust Basis claim boundary regresses or disappears.

Non-claims:

- Assay does not decide whether the model answer was correct.
- Assay does not import the full Promptfoo JSONL row, raw prompt, raw output,
  expected-value expansion, vars, provider payload, or run-level truth.
- Assay does not create an official Promptfoo integration claim.

### OpenFeature

OpenFeature remains the feature-flag evaluation surface. Assay only consumes
one bounded boolean `EvaluationDetails` export shape as receipt input.

Useful assurance framing:

- A reviewer can see that a supported runtime decision boundary exists in the
  evidence bundle.
- A reviewer can inspect the flag key, boolean value, value type, and, when
  present, the variant, reason, error code, source artifact reference, source
  artifact digest, reducer version, and import timestamp.
- A CI gate can block if the Trust Basis claim boundary regresses or disappears.

Non-claims:

- Assay does not decide whether the flag value was correct.
- Assay does not import targeting context, targeting key, rules, provider
  config, provider metadata, user identifiers, application state, or
  `error_message`.
- Assay does not create an official OpenFeature integration claim.

### CycloneDX ML-BOM

CycloneDX remains the inventory/BOM format. Assay only consumes selected
`machine-learning-model` component data as receipt input.

Useful assurance framing:

- A reviewer can see that a supported model inventory /
  provenance-reference boundary exists in the evidence bundle.
- A reviewer can inspect selected model component refs, bounded model/dataset
  and model-card refs, source artifact reference, source artifact digest,
  reducer version, and import timestamp.
- A CI gate can block if the Trust Basis claim boundary regresses or disappears.

Non-claims:

- Assay does not decide whether the model is safe or whether provenance is
  sufficient.
- Assay does not import the full BOM graph, vulnerabilities, licenses, pedigree,
  ancestors, full model-card body, dataset bodies, fairness/ethics sections, or
  compliance truth.
- Assay does not create an official CycloneDX integration claim.

## Harness Boundary

Assay Harness is deliberately generic in this layer.

Harness may:

- call Assay recipes or commands,
- preserve raw `assay.trust-basis.diff.v1` JSON,
- map Trust Basis regressions to CI exit codes,
- project raw diffs into Markdown and JUnit.

Harness must not:

- parse Promptfoo JSONL, OpenFeature JSONL, CycloneDX BOMs, or Assay receipt
  payloads,
- compare assertion values, flag decisions, model versions, dataset refs, or
  family-specific metadata,
- add its own Trust Basis claim logic.

## Importer-Only Lanes

Mastra score events and Pydantic case results are importer-only receipt lanes
in the current line. They are useful for importer boundary work, but they are
not public claim-visible families in this mapping.

They should stay out of assurance-facing family tables until a later accepted
claim slice explicitly changes the receipt family matrix and release notes.

## References

- [Evidence Receipts in Action](EVIDENCE-RECEIPTS-IN-ACTION.md)
- [Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory](EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md)
- [Receipt families](../reference/receipt-families.md)
- [Receipt family matrix](../reference/receipt-family-matrix.json)
- [Receipt schema registry](../reference/receipt-schemas/README.md)
