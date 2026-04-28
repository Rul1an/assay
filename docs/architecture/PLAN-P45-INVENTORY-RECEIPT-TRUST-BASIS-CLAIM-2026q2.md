# PLAN — P45 Inventory Receipt Trust Basis Claim (Q2 2026)

- **Date:** 2026-04-28
- **Owner:** Evidence / Trust Compiler
- **Status:** Execution slice
- **Scope:** Add one bounded Trust Basis claim for supported external inventory
  receipt evidence, starting with the P43 CycloneDX ML-BOM model-component
  receipt event.

## 1. Why this exists

P43 made the CycloneDX ML-BOM model-component compiler path real:

```text
CycloneDX ML-BOM model component
  -> assay evidence import cyclonedx-mlbom-model
  -> Assay EvidenceEvent receipt bundle
  -> assay evidence verify
  -> assay trust-basis generate
```

That proves inventory receipts are bundleable and readable. P45 is the next
compatibility step: make the supported inventory receipt boundary visible as a
named Trust Basis claim without importing BOM truth, model-card truth, dataset
truth, or compliance posture.

## 2. What P45 is

P45 adds:

- `external_inventory_receipt_boundary_visible`
- `source = external_inventory_receipt`
- `boundary = supported-external-inventory-receipt-events-only`
- Trust Card schema `4`, because the visible claim table changes

The claim is `verified` only when the bundle contains at least one supported
inventory receipt event whose payload matches the bounded v1 receipt predicate
exactly.

For the first slice, the only supported event is:

```text
assay.receipt.cyclonedx.mlbom_model_component.v1
```

with:

- `schema = "assay.receipt.cyclonedx.mlbom-model-component.v1"`
- `source_system = "cyclonedx"`
- `source_surface = "bom.components[type=machine-learning-model]"`
- bounded, reviewer-safe source artifact ref and digest
- `reducer_version` starting with
  `assay-cyclonedx-mlbom-model-component@`
- `imported_at` that parses as RFC3339 and has zero UTC offset
- bounded `model_component.bom_ref`
- bounded `model_component.name`
- optional bounded `version`, `publisher`, and `purl`
- optional bounded `dataset_refs[]` and `model_card_refs[]` as refs only

The CloudEvents `type` and the receipt payload `schema` are separate exact
identifiers. The event type uses the established event-name segment style
(`mlbom_model_component`), while the payload schema uses the receipt schema slug
(`mlbom-model-component`). P45 accepts only the exact strings above.

## 3. What P45 is not

P45 does not claim:

- the BOM is complete
- the model is safe, approved, licensed, compliant, vulnerable, or
  non-vulnerable
- the model card is correct
- the dataset refs are approved or sufficient
- the full CycloneDX graph was imported
- vulnerability, license, pedigree, metric, fairness, ethics, or compliance
  truth
- Harness inventory-drift semantics

The claim means only:

```text
the verified bundle contains at least one supported bounded inventory receipt
```

It does not mean:

```text
the upstream inventory statement is complete, correct, or sufficient
```

## 4. Predicate rule

The Trust Basis predicate must stay stricter than generic event presence.
Trust Basis claim support is narrower than generic EvidenceEvent acceptance:
future or wider inventory receipt events may verify as evidence, but they do
not satisfy this claim until the predicate is deliberately expanded.

`external_inventory_receipt_boundary_visible = verified` requires:

- supported inventory receipt event type
- exact supported source system and source surface
- bounded, reviewer-safe `source_artifact_ref`
- digest-shaped source artifact binding
- `imported_at` parseable as RFC3339 with zero UTC offset; serialized receipts
  should use `Z` form, and naive/local timestamps do not satisfy the predicate
- `reducer_version` starting with
  `assay-cyclonedx-mlbom-model-component@`
- bounded model-component object
- `dataset_refs[]` and `model_card_refs[]` as arrays of bounded string refs
  only
- no raw `modelCard` body, dataset body, BOM graph, vulnerability, license,
  pedigree, metrics, or other expanded inventory bodies in the receipt payload

In v1, bounded inventory strings are non-empty after trimming, serialized
without leading or trailing whitespace, no longer than 240 Unicode scalar
values, and contain no control characters. This applies to the source artifact
ref, model-component identity fields, and refs-only arrays.

Malformed, wider, or future-shaped inventory receipt payloads remain accepted by
evidence verify if the bundle contract allows them, but this Trust Basis claim
should stay `absent` until the predicate is deliberately widened.

## 5. Trust Card impact

Adding a claim row changes the Trust Card visible surface. P45 therefore bumps:

```text
TRUST_CARD_SCHEMA_VERSION = 4
```

The Trust Card remains a deterministic render of Trust Basis. It does not add a
second classifier, summary prose, aggregate score, compliance badge, or
inventory-specific interpretation layer.

## 6. Acceptance criteria

- Trust Basis always emits the new claim row.
- Ordinary bundles keep the claim `absent`.
- Supported P43 CycloneDX ML-BOM model-component receipt bundles classify it as
  `verified`.
- Receipt-like events that include model-card bodies, dataset bodies, expanded
  refs, or invalid provenance fields classify it as `absent`.
- Trust Card schema is bumped to `4`.
- Trust Card JSON and Markdown still render only the same claim rows plus frozen
  non-goals.
- CLI docs explain the claim boundary without describing BOM completeness,
  model safety, dataset approval, or compliance truth.

## 7. Sequencing

P45 comes after P43 and P44. It prepares the inventory family for generic
Trust Basis diff/gate/report flows.

The next likely slice is Harness-side validation that the existing generic
Trust Basis gate/report layer can carry this new claim family without learning
CycloneDX, BOM, model, dataset, or inventory semantics.
