# PLAN — P14c Mastra ScoreEvent Receipt Import (2026 Q2)

- **Date:** 2026-04-28
- **Owner:** Evidence / Product
- **Status:** Implemented importer slice
- **Scope:** Turn one bounded Mastra `ScoreEvent` / `ExportedScore`-derived
  artifact into one portable Assay evidence receipt bundle. This is an
  Assay-side compiler path, not a Mastra integration, partnership, exporter, or
  observability sink.

## 1. Why this exists

P14b recut the Mastra lane away from scorer definitions and experiment wrappers
toward the thinner exporter seam that Mastra maintainers pointed at:

```text
ObservabilityExporter -> onScoreEvent(ScoreEvent) -> ExportedScore
```

That recut now has two useful anchors:

- an Assay-side sample around `ScoreEvent` / `ExportedScore`
- Mastra observability docs now publicly expose `ObservabilityExporter` event
  callbacks, including `onScoreEvent(event: ScoreEvent)`

P14c is the next narrow step. It should make the score-event lane a real Assay
compiler path without claiming Mastra runtime truth, scoring truth, trace truth,
or dashboard truth.

## 2. Positioning rule

Use this sentence when explaining the lane:

> Mastra can surface score events through its observability exporter path.
> Assay can reduce selected score-event outcomes into portable evidence
> receipts.

Do not say:

- Assay integrates with Mastra
- Mastra supports Assay
- Assay verifies Mastra scores
- Assay imports Mastra observability
- Assay understands Mastra traces, scorers, or dashboards

This is one bounded evidence compiler lane over an existing upstream seam.

## 3. v1 input unit

P14c v1 should import one JSONL row per bounded score artifact derived from a
Mastra `ScoreEvent`.

The input row is not a raw callback dump. It is a reviewer-safe reduction over
`ScoreEvent.score` / `ExportedScore`. P14c v1 imports a reduced score-event
artifact in JSONL form, not a raw exporter callback payload.

Recommended input surface:

```json
{
  "schema": "mastra.score-event.export.v1",
  "framework": "mastra",
  "surface": "observability.score_event",
  "timestamp": "2026-04-28T12:00:00Z",
  "score_id_ref": "score_01h...",
  "scorer_id": "toxicity-check",
  "score": 0.98,
  "target_ref": "span_01h...",
  "score_source": "live",
  "trace_id_ref": "trace_01h...",
  "span_id_ref": "span_01h..."
}
```

The importer should accept JSONL rather than a single JSON document so the lane
can later stream multiple bounded score artifacts without changing the command
shape.

## 4. Required fields

P14c should require:

- `schema = "mastra.score-event.export.v1"`
- `framework = "mastra"`
- `surface = "observability.score_event"`
- `timestamp` as RFC3339 with UTC offset
- numeric `score`
- bounded `target_ref`

The input `surface` value intentionally matches the receipt `source_surface`
value to avoid contract drift between the reduced artifact and Assay receipt.

Preferred live-backed identity fields:

- bounded `score_id_ref`
- bounded `scorer_id`

These are the preferred canonical bounded identity fields for v1. Mastra's
public docs confirm the `onScoreEvent` exporter seam, while the stronger
`scoreId` / `scorerId` expectations are live-capture-backed rather than
docs-hard. A fresh 2026-04-30 capture on `@mastra/core` `1.29.1` /
`@mastra/observability` `1.10.2` proves that the current supported
`onScoreEvent` path carries both fields.

They remain optional in the released v1 reduced artifact for compatibility
with older captures and fixtures. Future schema versions may make this stricter
once the team is ready to intentionally break older reduced artifacts.

If `score_id_ref` is absent in v1, the reduced artifact still needs enough
bounded context to remain reviewable: `target_ref`, `timestamp`, numeric
`score`, and at least one scorer identity. `scorer_id` is strongly preferred
when naturally present; `scorer_name` remains a compatibility fallback rather
than the preferred compiler identity.

Why this is stricter than the older P14b sample:

- current type/live discovery exposes `scoreId` and `scorerId`
- a real receipt importer should prefer stable bounded identity over display
  labels
- `scorer_name` is useful for review, but should not be the primary identity
  for a compiler path once `scorer_id` is available

The importer should preserve these fields when present and tests should prove
they round-trip into receipts. Any later move from preferred to required should
be a deliberate schema-versioned tightening, not an accidental v1 drift.

## 5. Optional fields

P14c may preserve these bounded fields when naturally present:

- `scorer_name`
- `scorer_version`
- `score_source`
- `reason`
- `trace_id_ref`
- `span_id_ref`
- `score_trace_id_ref`
- `target_entity_type`
- `metadata_ref`

`metadata_ref` MUST be a bounded reviewer-safe string reference only. It is
non-resolving by default: no inline object, body expansion, URL requirement, or
dereference semantics are part of v1. Raw `metadata` or `correlationContext`
objects inline are malformed for v1.

`trace_id_ref`, `span_id_ref`, and `score_trace_id_ref` are anchors only. They
must not make this a trace import lane. These fields are optional reviewer aids
only and must not affect receipt validity or downstream claim semantics in v1.
`score_source`, `trace_id_ref`, `span_id_ref`, and `score_trace_id_ref` are
never part of the canonical identity of the receipt in v1.

## 6. v1 receipt payload

The Assay receipt should use one event per imported score artifact:

```text
type = "assay.receipt.mastra.score_event.v1"
```

Payload schema:

```json
{
  "schema": "assay.receipt.mastra.score_event.v1",
  "source_system": "mastra",
  "source_surface": "observability.score_event",
  "source_artifact_ref": "mastra-score-events.jsonl",
  "source_artifact_digest": "sha256:...",
  "reducer_version": "assay-mastra-score-event@0.1.0",
  "imported_at": "2026-04-28T12:00:00Z",
  "score_event": {
    "score_id_ref": "score_01h...",
    "scorer_id": "toxicity-check",
    "score": 0.98,
    "target_ref": "span_01h...",
    "timestamp": "2026-04-28T12:00:00Z",
    "score_source": "live",
    "trace_id_ref": "trace_01h...",
    "span_id_ref": "span_01h..."
  }
}
```

The importer should compute `source_artifact_digest` over the full input JSONL
file before reducing rows, following the Promptfoo/OpenFeature/CycloneDX
receipt lanes.

## 7. Exclusions

P14c v1 must not import:

- raw `metadata` bodies
- raw `correlationContext` bodies
- inline replacements for `metadata_ref`
- trace trees
- span payloads
- logs, metrics, or feedback events
- `addScoreToTrace(...)` legacy payloads as the primary seam
- scorer definitions
- scorer pipeline config
- prompts
- model outputs
- request or response bodies
- dashboard URLs
- experiment summaries
- score histograms or aggregate rollups

The lane is `ScoreEvent`-first. It is not observability-first.

Only bounded reference fields are allowed for metadata or correlation context
continuity. No raw body, object expansion, or callback-envelope import is part
of v1.

## 8. What the receipt does not claim

The receipt does not mean:

- the score is correct
- the scorer is reliable
- the model output was good or bad
- the Mastra runtime behaved correctly
- the trace/span anchor is complete
- the dashboard state is true
- the score should pass or fail a gate

The receipt means only:

```text
a selected Mastra score-event outcome was reduced into a bounded, provenance-bearing evidence receipt
```

## 9. CLI shape

Recommended command:

```bash
assay evidence import mastra-score-event \
  --input mastra-score-events.jsonl \
  --bundle-out mastra-score-receipts.tar.gz \
  --source-artifact-ref mastra-score-events.jsonl \
  --run-id mastra_score_event_import \
  --import-time 2026-04-28T12:00:00Z
```

Implementation should mirror the existing external receipt importers:

- strict streaming JSONL ingestion
- reduced score-event artifact input, not raw callback input
- full-file SHA-256 source digest
- one Assay `EvidenceEvent` receipt per score artifact
- direct `BundleWriter` output
- deterministic `--import-time` for fixtures
- fail closed on forbidden body fields

## 10. Evidence Contract posture

The implementation PR should add an experimental registry row for:

```text
assay.receipt.mastra.score_event.v1
```

Stable promotion requires the same governance bar as other event types:

- concrete payload section
- conformance tests
- type-specific payload invariant beyond envelope/hash validity

P14c should not add a Trust Basis claim. First prove:

- import works
- bundle verifies
- Trust Basis can read the bundle
- existing eval/decision/inventory claims remain unaffected

A later slice may decide whether score receipts belong under an existing family
or need a separate claim such as `external_score_receipt_boundary_visible`.

## 11. Tests

Minimum test set:

- valid score event JSONL imports into a verifiable bundle
- multiple rows produce multiple receipt events
- missing `target_ref` or `timestamp` fails closed
- present `score_id_ref` and `scorer_id` round-trip into receipts; missing
  `score_id_ref` remains accepted in v1 when the row still carries the minimum
  bounded review surface (`target_ref`, `timestamp`, numeric `score`, and at
  least one scorer identity)
- `score_source`, `trace_id_ref`, `span_id_ref`, and `score_trace_id_ref` do
  not participate in receipt identity or Trust Basis claim semantics
- non-numeric `score` fails closed
- raw `metadata` object fails closed
- raw `correlationContext` object fails closed
- `addScoreToTrace`-shaped row fails closed unless first reduced to the v1
  input shape
- Trust Basis generation succeeds and keeps existing external eval, decision,
  and inventory receipt claims absent

## 12. Outward posture

Do not open a new Mastra issue for P14c.

After P14c is on `main`, a low-pressure heads-up may be reasonable only if
there is natural context:

> Small downstream follow-up for context: I added an Assay-side receipt-import
> plan around bounded Mastra `ScoreEvent` / `ExportedScore` artifacts. It stays
> outside this repo and is framed as an external evidence-consumer path over
> the documented exporter callback surface, not as an integration or
> partnership claim.

No ask. No tag. No "support" language.

## 13. Non-goals

P14c does not:

- implement a Mastra exporter
- run Mastra
- parse full Mastra traces
- import logs, metrics, or feedback
- define score correctness
- define scorer reliability
- add Harness recipe support
- add SARIF/JUnit projection
- add Trust Basis score-claim semantics
- make any upstream contribution to Mastra

## 14. References

- [P14b Mastra ScoreEvent / ExportedScore Evidence Interop](./PLAN-P14B-MASTRA-SCORE-EVENT-EVIDENCE-2026q2.md)
- [Mastra ScoreEvent evidence sample](../../examples/mastra-score-event-evidence/README.md)
- [Mastra PR #15757 — docs: document observability exporter event callbacks](https://github.com/mastra-ai/mastra/pull/15757)
- [Mastra ObservabilityExporter interface docs](https://mastra.ai/reference/observability/tracing/interfaces)
