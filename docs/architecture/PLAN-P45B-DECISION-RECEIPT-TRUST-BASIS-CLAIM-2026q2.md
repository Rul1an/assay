# PLAN — P45b Decision Receipt Trust Basis Claim (Q2 2026)

- **Date:** 2026-04-28
- **Owner:** Evidence / Trust Compiler
- **Status:** Execution slice
- **Scope:** Add one bounded Trust Basis claim for supported external decision
  receipt evidence, starting with the P41 OpenFeature boolean
  `EvaluationDetails` receipt event.

## 1. Why this exists

P41 made the OpenFeature boolean `EvaluationDetails` compiler path real:

```text
OpenFeature EvaluationDetails<boolean> JSONL
  -> assay evidence import openfeature-details
  -> Assay EvidenceEvent receipt bundle
  -> assay evidence verify
  -> assay trust-basis generate
```

That proved decision receipts are bundleable and readable. P45b is the next
compatibility step: make the supported decision receipt boundary visible as a
named Trust Basis claim without importing provider, targeting, rule, context, or
application-correctness truth.

## 2. What P45b is

P45b adds:

- `external_decision_receipt_boundary_visible`
- `source = external_decision_receipt`
- `boundary = supported-external-decision-receipt-events-only`
- Trust Card schema `5`, because the visible claim table changes

The claim is `verified` only when the bundle contains at least one supported
decision receipt event whose payload matches the bounded v1 receipt predicate
exactly.

For the first slice, the only supported event is:

```text
assay.receipt.openfeature.evaluation_details.v1
```

with:

- `schema = "assay.receipt.openfeature.evaluation_details.v1"`
- `source_system = "openfeature"`
- `source_surface = "evaluation_details.boolean"`
- bounded, reviewer-safe source artifact ref and digest
- `reducer_version` starting with
  `assay-openfeature-evaluation-details@`
- `imported_at` that parses as RFC3339 and has zero UTC offset
- bounded `decision.flag_key`
- `decision.value_type = "boolean"`
- boolean `decision.value`
- optional bounded `decision.variant`, `decision.reason`, and
  `decision.error_code`

P45b treats OpenFeature `EvaluationDetails` as a bounded decision result
surface. It does not treat that result as application, provider, targeting, or
rule truth.

## 3. What P45b is not

P45b does not claim:

- the flag decision was correct
- the provider behaved correctly
- the targeting rules are correct
- the flag configuration is complete or true
- the application behavior controlled by the flag is safe
- the evaluation context, targeting key, provider metadata, flag metadata, or
  `error_message` was imported
- Harness decision-drift semantics

The claim means only:

```text
the verified bundle contains at least one supported bounded decision receipt
```

It does not mean:

```text
the upstream decision statement is correct, sufficient, or policy-compliant
```

## 4. Predicate rule

The Trust Basis predicate must stay stricter than generic event presence. Trust
Basis claim support is narrower than generic EvidenceEvent acceptance: future or
wider decision receipt events may verify as evidence, but they do not satisfy
this claim until the predicate is deliberately expanded.

`external_decision_receipt_boundary_visible = verified` requires:

- supported decision receipt event type
- exact supported source system and source surface
- bounded, reviewer-safe `source_artifact_ref`
- digest-shaped source artifact binding
- `imported_at` parseable as RFC3339 with zero UTC offset; serialized receipts
  should use `Z` form, and naive/local timestamps do not satisfy the predicate
- `reducer_version` starting with
  `assay-openfeature-evaluation-details@`
- bounded decision object
- boolean-only `value_type` and `value`
- no evaluation context, targeting key, provider config, provider metadata,
  flag metadata, targeting rules, user identifiers, or `error_message` in the
  receipt payload

In v1, bounded decision strings are non-empty after trimming, serialized without
leading or trailing whitespace, no longer than the field-specific cap, and
contain no newline, carriage return, quote, backtick, or inline JSON delimiters.
`flag_key` has a 200 Unicode scalar value cap; optional decision strings have a
120 Unicode scalar value cap.

Malformed, wider, or future-shaped decision receipt payloads remain accepted by
evidence verify if the bundle contract allows them, but this Trust Basis claim
should stay `absent` until the predicate is deliberately widened.

## 5. Trust Card impact

Adding a claim row changes the Trust Card visible surface. P45b therefore bumps:

```text
TRUST_CARD_SCHEMA_VERSION = 5
```

The Trust Card remains a deterministic render of Trust Basis. It does not add a
second classifier, summary prose, aggregate score, compliance badge, or
decision-specific interpretation layer.

## 6. Acceptance criteria

- Trust Basis always emits the new claim row.
- Ordinary bundles keep the claim `absent`.
- Supported P41 OpenFeature boolean decision receipt bundles classify it as
  `verified`.
- Promptfoo eval receipts remain non-decision receipts.
- CycloneDX inventory receipts remain non-decision receipts.
- Receipt-like events that include context, metadata, rules, `error_message`, or
  non-boolean values classify it as `absent`.
- Trust Card schema is bumped to `5`.
- Trust Card JSON and Markdown still render only the same claim rows plus frozen
  non-goals.
- CLI docs explain the claim boundary without describing flag correctness,
  provider correctness, targeting-rule correctness, or application safety.

## 7. Sequencing

P45b comes after P41 and P45. It closes the claim-family symmetry needed before
the larger external story:

```text
Promptfoo  -> eval outcome receipts
OpenFeature -> runtime decision receipts
CycloneDX  -> inventory/provenance receipts
```

The next likely slice is Harness-side fixture/schema/script hygiene that proves
the existing generic Trust Basis gate/report layer can carry the expanded claim
set without learning Promptfoo, OpenFeature, or CycloneDX semantics.
