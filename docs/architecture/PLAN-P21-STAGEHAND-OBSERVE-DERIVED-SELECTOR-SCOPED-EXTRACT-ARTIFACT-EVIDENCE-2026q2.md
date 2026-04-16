# PLAN — P21 Stagehand Observe-Derived Selector-Scoped Extract Artifact Evidence Interop (2026 Q2)

- **Date:** 2026-04-16
- **Owner:** Evidence / Product
- **Status:** Discovery / sample lane
- **Scope (current repo state):** Define one bounded Stagehand-adjacent lane
  centered on one observe-derived selector anchor plus one selector-scoped
  extract result. This plan does **not** propose broad Stagehand support,
  browser trace import, session export, or page snapshot truth.

## 1. Why `P21` should exist

Stagehand is now a serious, active browser-agent repo with a clear product
shape and a clean split between:

- `observe()` for structured suggested actions
- `extract()` for structured data extraction
- `act()` for action execution
- `agent()` for broader multi-step browser-agent behavior

That matters because Assay does **not** need the whole browser-agent system.
It only needs the smallest honest external-consumer seam that can produce a
bounded, reviewable artifact.

The strongest candidate seam is:

- one selector discovered by `observe()`
- one `extract()` call scoped to that selector
- one small structured extract result

## 2. Why this seam is timely

This seam is not hypothetical. It is already alive upstream.

The Stagehand docs explicitly position `observe()` as a way to discover
structured actions and then pass those selectors into `extract()` to reduce
token usage and improve accuracy.

The public repo also shows real design pressure around exactly this area:

- `#1823` asks for harder scoping around `act`, `observe`, and `extract`
- `#1904` asks for `selectorAll`, which is a signal that selector cardinality
  and first-match behavior still matter
- `#1555` asks for more stable selectors, which is a signal that selector
  identity is useful but not yet a timeless truth surface

That is exactly the kind of upstream posture Assay can work with:

- there is already a named seam
- there is already public design pressure on it
- but it is still small enough to study without swallowing the whole product

## 3. Hard positioning rule

This lane must stay smaller than the upstream ecosystem name.

Normative framing:

> `P21` v1 claims only bounded selector-scoped extraction evidence derived from
> one observe-discovered selector anchor. It does not claim DOM truth, page
> snapshot truth, Stagehand execution completeness, or browser-agent truth.

Common anti-overclaim sentence:

> We are not asking Assay to inherit Stagehand page understanding, execution
> semantics, or browser-state semantics as truth.

## 4. Recommended `P21` seam

The correct lane name is intentionally small:

**Stagehand Observe-Derived Selector-Scoped Extract Artifact Lane**

The recommended v1 seam is:

- one bounded instruction label for `observe()`
- one observe-derived selector anchor
- one explicit selector source label fixed to `observe`
- one bounded instruction label for `extract()`
- one selector-scoped structured extract result

In v1 this is a contract rule, not a soft preference:

- `selector_source` must be `observe`
- hand-authored selectors are out of scope for the first sample
- recursively scoped observation trees are out of scope for the first sample

## 5. Non-goals

`P21` should reject broader Stagehand surfaces on purpose.

Out of scope:

- raw page snapshots
- full `observe()` action lists
- `act()` execution truth
- `agent()` task-completion truth
- screenshots or video
- browser traces or session exports
- browser runtime completeness claims

## 6. Upstream-reality caveats we must preserve

Three cautions matter immediately:

### 6.1 Selector identity is useful, not timeless

`P21` must not claim:

- that Stagehand selectors are permanent ids
- that one selector is stable across layout changes
- that one selector uniquely and permanently names one semantic object

For Assay, the selector is a bounded observed anchor only.

### 6.2 Scoped extraction is bounded, not page truth

Selector-scoping is useful, but `P21` must not turn that into:

- proof that all relevant page content was in scope
- proof that the extracted result is complete for the whole page
- proof that the scoped subtree itself is the whole truth we care about

This lane claims one bounded scoped extraction result, not completeness.

### 6.3 First-match / cardinality semantics are not closed

Because upstream discussion already includes `selectorAll` pressure, `P21`
must avoid pretending that today's selector behavior already settles multi-hit
or list semantics.

So v1 stays on:

- one selector anchor
- one scoped result bag

and avoids:

- multi-selector lists
- full observe action arrays
- completeness claims over repeated elements

## 7. Recommended v1 artifact contract

Use one frozen serialized artifact derived from the selector-scoped Stagehand
extract lane.

Required fields:

- `schema`
- `framework`
- `surface`
- `timestamp`
- `observe_instruction`
- `extract_instruction`
- `selector_ref`
- `selector_source`
- `selector_kind`
- `result`

Optional fields:

- `scope_hint`
- `result_schema_ref`
- `cache_status`
- `page_ref`
- `run_ref`
- `metadata_ref`

Contract rules:

- `selector_source` must be `observe`
- `selector_kind` may be `xpath`, `css`, or `other`
- `result` must be one small structured bag, not an array of bags
- v1 accepts only a plain bounded structured extract result, not a broader
  page-model export

Failure-fixture rule:

> A valid failure fixture should show a bounded but incomplete or empty
> extraction result. It should not imply a Stagehand-native confidence or
> ranking model unless a live capture proves one.

Malformed-cardinality rule:

> Any frozen artifact containing multiple selectors, a full `observe()` action
> list, or multiple structured result bags should be malformed for v1.

## 8. Current discovery outcome

`P21` is no longer just a plan-only lane. We now have one small runtime-backed
local probe.

What was run:

- Stagehand in `LOCAL` mode
- local Chrome launch
- one tiny HTML page served via `data:` URL
- one `observe()` instruction aimed at an invoice-summary card
- one `extract()` instruction scoped to the selector returned by `observe()`

What succeeded:

- a runtime-backed local probe succeeded using the public exported
  `AISdkClient` with scripted LLM responses
- `observe()` returned one selector-bearing action
- that selector was:
  `xpath=/html[1]/body[1]/div[1]/section[1]`
- `extract()` scoped to that selector returned a small structured result bag:
  `invoice_number=INV-2048`, `total_due=EUR 128.40`

What remains open:

- a provider-live local probe against `ollama/llama3.2:3b` did **not** pass
- Stagehand and the local browser launched correctly, but the local Ollama
  runner failed on the structured `observe()` prompt with an internal runner
  error

So the checked-in P21 sample should be described honestly as:

- runtime-backed
- selector-scoped
- observe-derived
- but still **pre-proof on provider-live model capture**

## 9. Sample path recommendation

The first repo sample for this lane should live at:

`examples/stagehand-selector-scoped-extract-evidence/`

Recommended contents:

- `README.md`
- `map_to_assay.py`
- `fixtures/valid.stagehand.json`
- `fixtures/failure.stagehand.json`
- `fixtures/malformed.stagehand.json`
- generated `*.assay.ndjson` outputs for the passing fixtures

## 10. Discovery gate

`P21` should not widen before one more proof step is done.

Current closure rule:

> The sample lane is ready and reviewable when one runtime-backed
> observe-derived selector plus one selector-scoped extract result have been
> frozen honestly, with provider-live status called out explicitly.

Next proof rule:

> `P21` is not provider-live-closed until one supported live model path emits a
> passing observe-derived selector plus scoped extract result without the
> scripted LLM fallback.

## 11. Recommended outward posture

Do **not** position this as "Stagehand support".

The right outward posture is:

- selector-scoped extraction-first
- one observe-derived selector anchor
- one bounded extract result
- no DOM truth
- no snapshot truth
- no full `observe()` planning truth

## 12. One-sentence working label

> `P21` is a bounded observe-derived selector-scoped extract evidence lane,
> runtime-backed locally, but still pre-proof on provider-live model capture.
