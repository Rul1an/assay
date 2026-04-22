Date: 2026-04-22
Owner: Evidence / External Interop
Status: Planning lane
Scope (current repo state): Explore one bounded LangWatch-adjacent evidence lane built around a single custom evaluation attached to a single span through the public LangWatch SDK surface. This plan is for the smallest honest external-consumer seam only. It does not propose broad LangWatch support, offline evaluation-run support, dataset support, trace export support, or prompt-management support.

## 1. Why this plan exists

LangWatch is a strong adjacent candidate because it is both publicly active and explicit about evaluation as a first-class surface. But the repo and product surface are broad: traces, datasets, offline evaluation, real-time evaluation, prompt management, annotations, and scenario testing all sit nearby.

The first Assay wedge therefore needs to stay smaller than "LangWatch interop" or "LangWatch evaluation support."

The strongest small seam visible in the public docs is the custom evaluator path attached to the current span:

- one custom evaluation
- one current span target
- one bounded result bag

The LangWatch docs explicitly show `add_evaluation(...)` on the current span, with:

- required `name`
- optional `passed`
- optional `score`
- optional `label`
- optional `details`

and they explicitly require that at least one of `passed`, `score`, or `label` be present.

That makes it a good P25 candidate.

## 2. What this plan is and is not

This plan is for:

- one span-linked custom evaluation
- one bounded result bag
- one small external-consumer artifact derived from that evaluation
- one discovery pass over the public SDK path and first surfaced result view

This plan is not for:

- full LangWatch tracing support
- offline evaluation session support
- evaluator-run arrays or batch exports
- dataset truth
- annotation queue truth
- prompt-management truth
- scenario or simulation truth
- dashboard or review-queue state

## 3. Hard positioning rule

P25 v1 claims only one bounded LangWatch custom span evaluation artifact as imported external evaluation signal evidence. It does not claim trace truth, dataset truth, evaluation-session truth, annotation truth, prompt-management truth, or LangWatch platform truth.

That means:

- LangWatch remains the source of the observed evaluation signal
- Assay imports only the smallest honest span-linked result surface
- Assay does not inherit broader LangWatch workflow semantics as truth

## 4. Recommended seam

The first seam should stay on exactly one move:

- attach one custom evaluation to one current span through the public `add_evaluation(...)` path

Not:

- `evaluation.log(...)` across a broader offline evaluation session
- `evaluation.run(...)` with built-in evaluators and server-side result envelopes
- run-level summaries
- dataset row tracking
- raw trace export

This keeps the lane on the smallest named LangWatch evaluation signal rather than the broader evaluation product surface.

## 5. Canonical v1 artifact thesis

The reduced artifact should stay on a single span target with a single evaluation result bag:

```json
{
  "schema": "langwatch.custom-span-evaluation.export.v1",
  "framework": "langwatch",
  "surface": "custom_span_evaluation",
  "entity_kind": "span",
  "entity_id_ref": "span_opaque_id",
  "evaluation_name": "correctness",
  "result": {
    "passed": true,
    "score": 0.92,
    "label": "correct",
    "details": "Short bounded explanation."
  },
  "timestamp": "2026-04-22T12:00:00Z"
}
```

Optional reviewer aids, only if naturally present in the first honest surfaced representation:

- `trace_id_ref`
- `sdk_language`

Not allowed in v1:

- raw trace payloads
- dataset identifiers
- offline evaluation session identifiers
- evaluation arrays or batch wrappers
- prompt metadata
- arbitrary data bags
- queue/reviewer workflow fields
- raw platform response wrappers

## 6. Field boundaries

### 6.1 `entity_kind`

For v1, the only allowed value is:

- `span`

This keeps the lane on the current span custom-evaluation seam and prevents drift into thread-, trace-, or session-level evaluation surfaces.

### 6.2 `entity_id_ref`

This is the bounded anchor to the evaluated span.

It must remain:

- opaque
- short
- non-resolving

It must not become:

- a full span export
- a trace reconstruction surface
- a dashboard URL

### 6.3 `evaluation_name`

This is the canonical Assay-side name for LangWatch's required `name` field.

It should stay:

- required
- short
- stable enough to identify the evaluation kind inside the artifact

It must not become:

- a prompt body
- an evaluator configuration blob
- a human review thread title

### 6.4 `result`

The reduced result bag should remain smaller than LangWatch's broader evaluation and trace surfaces.

For v1:

- at least one of `passed`, `score`, or `label` must be present
- `details` is optional and bounded
- empty or whitespace-only `details` should be omitted or treated as malformed

This is directly aligned with the documented `add_evaluation(...)` contract rather than a richer invented artifact.

### 6.5 `passed`

This is an observed boolean result signal when present.

It must not be treated as:

- an Assay-side judgment
- universal evaluator truth
- a guarantee about downstream policy decisions

### 6.6 `score`

This is an observed scalar evaluation signal when present.

It must remain:

- numeric
- bounded to the first surfaced shape actually observed
- uninterpreted beyond being a score value emitted by LangWatch-side evaluation code

It must not be treated as:

- normalized universal score semantics
- cross-evaluator comparability truth
- a ranking contract

### 6.7 `label`

This is an observed categorical result signal when present.

It must remain:

- short
- bounded
- reviewer-readable

It must not become:

- a taxonomy import
- a queue-routing contract
- a platform-wide category system claim

### 6.8 `details`

This is optional reviewer support only.

It must remain:

- short
- bounded
- explanatory rather than transcript-like

It must not become:

- chain-of-thought import
- full evaluator reasoning
- raw trace excerpt
- prompt/completion content dump

## 7. Observed vs derived rule

P25 v1 should remain almost entirely observed.

Observed:

- span-linked evaluation exists
- `name`
- any present `passed`
- any present `score`
- any present `label`
- any present bounded `details`
- surfaced span anchor and timestamp if actually present

Derived:

- renaming upstream `name` into canonical `evaluation_name`
- any strictly minimal field normalization required to freeze the artifact

The plan must not invent a derived continuity, ranking, or reconciliation layer.

## 8. Cardinality rule

This lane is for exactly one evaluation object attached to exactly one span.

Therefore v1 artifacts should be malformed if they contain:

- multiple evaluations
- evaluation arrays
- multi-span wrappers
- batched run results
- trace-wide export envelopes

No partial import of larger evaluation bundles should be allowed in v1.

## 9. Discovery gate

P25 should not advance on docs snippets alone.

Required first proof:

- emit one real custom evaluation through the public SDK `add_evaluation(...)` path
- capture the raw emitted input to that call
- capture the first public surfaced representation that shows the evaluation back on the span
- compare emitted and surfaced fields before freezing any reduced artifact

If the surfaced representation is only visible through a trace view or exported trace payload, raw discovery notes must keep that context separate from the reduced artifact.

## 10. Initial malformed rules

Artifacts should be malformed if they contain:

- no `evaluation_name`
- no `result`
- a `result` with none of `passed`, `score`, or `label`
- empty or whitespace-only `details`
- raw dataset or evaluation-session identifiers
- raw trace export fields
- prompt-management or queue workflow fields
- evaluation arrays or batch wrappers
- dashboard URLs in id fields

## 11. Repository deliverables for first execution

If discovery validates the seam, the first concrete P25 lane should include:

- a formal example directory
- one live discovery note with raw emitted vs surfaced field presence
- one small mapper
- valid, failure, and malformed fixtures
- generated placeholder NDJSON outputs for valid cases

Suggested layout:

```text
examples/
  langwatch-custom-span-evaluation-evidence/
    README.md
    map_to_assay.py
    capture_probe.py
    discovery/
      FIELD_PRESENCE.md
    fixtures/
      valid.langwatch.json
      failure.langwatch.json
      malformed.langwatch.json
      valid.assay.ndjson
      failure.assay.ndjson
```

## 12. Success criteria

This plan succeeds when:

- Assay has one credible LangWatch-adjacent seam that is smaller than LangWatch platform truth
- the lane stays on a single span-linked custom evaluation
- the result bag remains bounded and reviewer-readable
- discovery proves emitted vs surfaced shape before any contract freeze

## 13. Final judgment

P25 should be a custom-evaluation-first LangWatch lane: one current span, one bounded evaluation signal, and nothing broader.
