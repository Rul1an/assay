# PLAN — P33 External Eval Receipt Trust Basis Claim (Q2 2026)

- **Date:** 2026-04-27
- **Owner:** Evidence / Trust Compiler
- **Status:** Execution slice
- **Scope:** Add one bounded Trust Basis claim for supported external evaluation receipt evidence, starting with the P31 Promptfoo assertion-component receipt event.

## 1. Why this exists

P31 made the Promptfoo compiler path real:

```text
Promptfoo CLI JSONL component result
  -> assay evidence import promptfoo-jsonl
  -> Assay EvidenceEvent receipt bundle
```

P32 then proved those bundles are not dead-end files:

```text
receipt bundle
  -> assay evidence verify
  -> assay trust-basis generate
```

P33 is the next deliberate compatibility step. It adds one Trust Basis row that
lets reviewers see that a supported external evaluation receipt boundary is
present in the verified bundle.

## 2. What P33 is

P33 adds:

- `external_eval_receipt_boundary_visible`
- `source = external_evidence_receipt`
- `boundary = supported-external-eval-receipt-events-only`
- Trust Card schema `3`, because the visible claim table changes

The claim is `verified` only when the bundle contains a supported receipt event
whose payload still matches the bounded v1 receipt shape.

For the first slice, the only supported event is:

```text
assay.receipt.promptfoo.assertion_component.v1
```

with:

- `schema = "assay.receipt.promptfoo.assertion-component.v1"`
- `source_system = "promptfoo"`
- `source_surface = "cli-jsonl.gradingResult.componentResults"`
- reviewer-safe source artifact ref and digest
- reducer version
- UTC RFC3339 import timestamp
- `assertion_type = "equals"`
- `result.pass`
- binary `result.score`
- optional bounded `result.reason`

## 3. What P33 is not

P33 does not claim:

- Promptfoo run pass/fail truth
- model-output correctness
- prompt/output/expected-value truth
- Promptfoo config, provider, token, cost, stats, or red-team truth
- broad external-evaluation support
- Harness baseline/candidate regression semantics

The claim means only:

```text
the verified bundle contains at least one supported bounded external eval receipt
```

It does not mean:

```text
the external evaluation result is correct or sufficient
```

## 4. Predicate rule

The Trust Basis predicate must stay stricter than generic event presence.

`external_eval_receipt_boundary_visible = verified` requires:

- supported receipt event type
- exact supported source system and source surface
- digest-shaped source artifact binding
- UTC RFC3339 import timestamp
- supported reducer-version prefix
- supported assertion type
- bounded result object
- no top-level raw Promptfoo payload fields in the receipt payload

Malformed, wider, or future-shaped receipt payloads remain accepted by evidence
verify if the bundle contract allows them, but this Trust Basis claim should
stay `absent` until the predicate is deliberately widened.

## 5. Trust Card impact

Adding a claim row changes the Trust Card visible surface. P33 therefore bumps:

```text
TRUST_CARD_SCHEMA_VERSION = 3
```

The Trust Card remains a deterministic render of Trust Basis. It does not add a
second classifier, summary prose, aggregate score, or badge.

## 6. Acceptance criteria

- Trust Basis always emits the new claim row.
- Ordinary bundles keep the claim `absent`.
- Supported P31 Promptfoo receipt bundles classify it as `verified`.
- Receipt-like events that include raw Promptfoo payload fields classify it as
  `absent`.
- Trust Card schema is bumped to `3`.
- Trust Card JSON and Markdown still render only the same claim rows plus frozen
  non-goals.
- CLI docs explain the claim boundary without describing Promptfoo correctness
  or run success.

## 7. Sequencing

P33 comes after P32 and before any Harness compare work.

The next likely slice after P33 is Harness-level comparison over compiled
receipts. Harness should compare Assay receipt/trust artifacts; it should not
learn Promptfoo JSONL parsing semantics directly.
