# PLAN — P14d Mastra Score Receipt Trust Basis Readiness Freeze (Q2 2026)

- **Date:** 2026-05-02
- **Owner:** Evidence / Trust Compiler
- **Status:** Execution freeze slice
- **Scope:** Freeze the semantic boundary for Mastra ScoreEvent receipts before
  any score-derived signal becomes Trust Basis claim-visible. This does not add
  a claim, Trust Card row, Harness recipe, or Mastra integration surface.

## 1. Why this exists

P14c made the Mastra ScoreEvent receipt lane operational:

```text
bounded Mastra ScoreEvent JSONL
  -> assay evidence import mastra-score-event
  -> verifiable Assay receipt bundle
  -> assay trust-basis generate
```

That proves the import layer works. It does not prove that score receipts are
ready to carry Trust Basis meaning.

The open question after P14c is therefore not:

```text
Can Assay import Mastra score receipts?
```

The open question is:

```text
What Trust Basis meaning may a score receipt ever carry?
```

P14d exists to keep that decision explicit before the score lane gains public
claim weight.

## 2. Decision

`assay.receipt.mastra.score_event.v1` remains importer-only for the current
line.

The receipt is schema-covered, bundleable, verifiable, and readable by the
Trust Basis path. It does not feed any current Trust Basis claim family:

- not `external_eval_receipt_boundary_visible`
- not `external_decision_receipt_boundary_visible`
- not `external_inventory_receipt_boundary_visible`

The receipt family matrix must continue to record:

```json
{
  "event_type": "assay.receipt.mastra.score_event.v1",
  "trust_basis_claim": null
}
```

This is the safe status until score-receipt claim semantics are deliberately
accepted and tested in a later compatibility-expanding slice.

## 3. Why not claim-visible now

Score receipts are tempting because the importer is real and Mastra maintainer
guidance made the seam unusually strong:

- `ScoreEvent` / `onScoreEvent` is the forward path for score export.
- `addScoreToTrace(...)` is legacy / migration context.
- `scoreId` has shipped and is now represented as bounded `score_id_ref` when
  naturally present.

That is enough for a receipt lane. It is not enough for a Trust Basis claim.

The word "score" is easy to overread as:

- quality proof
- safety proof
- pass/fail proof
- evaluator truth
- scorer reliability
- Mastra runtime truth
- threshold or gate outcome

P14d keeps those meanings out of the claim table.

## 4. Current receipt meaning

Today, a valid Mastra score receipt means only:

```text
a selected Mastra score-event outcome was reduced into a bounded, provenance-bearing evidence receipt
```

It does not mean:

- the score is correct
- the scorer is reliable
- the target passed
- the target is safe
- the Mastra runtime behaved correctly
- the trace or span anchor is complete
- the score should pass or fail an Assay or Harness gate

## 5. Reserved future claim candidate

If this lane later becomes claim-visible, the likely candidate claim is:

```text
external_score_receipt_boundary_visible
```

The maximum acceptable v1 meaning for that claim is narrow:

```text
the verified bundle contains at least one supported bounded external score receipt
```

For Mastra, the first supported event would be:

```text
assay.receipt.mastra.score_event.v1
```

with the existing bounded receipt predicates from P14c:

- exact event type
- exact schema, source system, and source surface
- reviewer-safe source artifact ref
- digest-shaped source artifact binding
- supported reducer version
- UTC RFC3339 import timestamp
- numeric score
- bounded target reference
- at least one bounded scorer identity
- no raw metadata, correlation context, traces, spans, logs, metrics, feedback,
  prompts, request/response bodies, scorer configs, dashboard state, or legacy
  callback envelope

Even then, the candidate claim must not say the score is correct, sufficient,
safe, trusted, passed, or failed.

## 6. P14e readiness bar

A later P14e may add `external_score_receipt_boundary_visible` only if the
compatibility cost is accepted deliberately.

Minimum readiness requirements:

- exact claim id
- exact Trust Basis source and boundary strings
- exact supported event types
- field-level predicate for valid supported score receipts
- negative examples for what the claim does not mean
- fixture where the claim becomes `verified`
- fixture where malformed or wider score-like events remain `absent`
- Trust Card schema impact and migration note
- Harness decision: either a recipe is added intentionally, or Harness remains
  explicitly unchanged
- consumer compatibility note for code that keys by `claim.id`

Until those are present, importer-only is the correct state.

## 7. Harness posture

Harness does not change in P14d.

Current Harness gate/report semantics consume `assay.trust-basis.diff.v1` over
the existing Trust Basis claim set. Since Mastra score receipts remain
`trust_basis_claim: null`, there is no Harness recipe, report field, JUnit
projection, or baseline/candidate comparison rule to add.

If P14e later adds a score receipt claim, Harness can be considered after the
Trust Basis and Trust Card compatibility decision is already made. Harness
should compare compiled Assay artifacts; it should not learn Mastra ScoreEvent
payload semantics directly.

## 8. Acceptance criteria

P14d is complete when:

- docs state that `assay.receipt.mastra.score_event.v1` remains importer-only
- the receipt family matrix still has `trust_basis_claim: null` for Mastra
  score receipts
- the P14c test posture remains true: Mastra score receipts do not mutate eval,
  decision, or inventory receipt claims
- the possible future claim `external_score_receipt_boundary_visible` is named
  only as a reserved candidate, not as a shipped claim
- P14e readiness requirements are recorded before any claim-visible work starts
- Harness is explicitly recorded as unchanged for this freeze slice
- release notes call this a semantic freeze, not feature expansion

## 9. Outward posture

No new Mastra comment is needed for P14d.

The Mastra thread already established the important seam:

- `ScoreEvent` / `onScoreEvent` is the forward path
- `addScoreToTrace(...)` is legacy context
- `scoreId` has shipped

P14d is internal product semantics. It should not be framed as upstream support,
an integration claim, or a request for new Mastra behavior.

## 10. References

- [P14b Mastra ScoreEvent / ExportedScore Evidence Interop](./PLAN-P14B-MASTRA-SCORE-EVENT-EVIDENCE-2026q2.md)
- [P14c Mastra ScoreEvent Receipt Import](./PLAN-P14C-MASTRA-SCOREEVENT-RECEIPT-IMPORT-2026q2.md)
- [Mastra ScoreEvent evidence sample](../../examples/mastra-score-event-evidence/README.md)
- [Receipt family matrix](../reference/receipt-family-matrix.json)
- [Mastra ObservabilityExporter interface docs](https://mastra.ai/reference/observability/tracing/interfaces)
