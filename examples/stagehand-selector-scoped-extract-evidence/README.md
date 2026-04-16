# Stagehand Selector-Scoped Extract Evidence Sample

This example turns one tiny frozen artifact derived from a Stagehand
observe-derived selector plus one selector-scoped extract result into bounded,
reviewable external evidence for Assay.

It is intentionally small:

- start from one observe-derived selector anchor
- keep the sample to one valid artifact, one failure artifact, and one
  malformed case
- map the two good artifacts into Assay-shaped placeholder envelopes
- keep selector source, selector kind, and one small structured result bag at
  the center
- keep snapshots, full `observe()` action lists, `act()`, `agent()`, and
  broader browser/runtime truth out of Assay

## What is in here

- `map_to_assay.py`: turns one tiny Stagehand selector-scoped extract artifact
  into an Assay-shaped placeholder envelope
- `fixtures/valid.stagehand.json`: one bounded successful selector-scoped
  extract sample
- `fixtures/failure.stagehand.json`: one bounded incomplete selector-scoped
  extract sample
- `fixtures/malformed.stagehand.json`: one malformed import case
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with a fixed
  import time

## Why this seam

This sample treats one observe-derived selector anchor plus one
selector-scoped extract result as the best first Stagehand lane hypothesis for
Assay.

That keeps the first slice on bounded extraction evidence only. It does not
turn the sample into:

- a Stagehand browser-state import path
- a Stagehand runtime-completeness claim
- a raw snapshot lane
- a full `observe()` planning lane
- an `act()` or `agent()` lane

The checked-in fixtures are deliberately frozen and smaller than the full
Stagehand model. Stagehand itself is broader than this sample. V1 keeps the
evidence boundary honest without pretending that Assay now models Stagehand as
a browser-agent runtime.

The top-level `schema`, `framework`, and `surface` fields in these fixtures
are sample wrapper metadata. They help identify the frozen artifact and the
seam hypothesis, but they are not a claim that Stagehand itself ships one
canonical wrapper with those same labels.

## Current discovery seam

This sample is grounded in one small runtime-backed local probe.

What that means in practice:

- Stagehand ran in `LOCAL` mode against a local Chrome launch
- one tiny HTML page was loaded via `data:` URL
- one `observe()` instruction returned a selector-bearing action
- one `extract()` instruction scoped to that selector returned a small result
  bag

The current probe-backed selector was:

- `xpath=/html[1]/body[1]/div[1]/section[1]`

The current probe-backed result bag was:

- `invoice_number=INV-2048`
- `total_due=EUR 128.40`

Important honesty line:

- the runtime path is real
- the current local model-provider path is **not** yet proven live

The provider-live probe against local `ollama/llama3.2:3b` did not pass. The
browser and Stagehand came up, but the local Ollama runner failed on the
structured `observe()` prompt. So the checked-in fixtures are currently
runtime-backed but still pre-proof on provider-live model capture.

## Why the result bag is smaller than generic extraction

Stagehand extraction can be richer than this sample allows.

This sample deliberately keeps `result` to one small flat structured object
with bounded scalar values only. It does **not** import:

- nested result bags
- arrays of result objects
- confidence or ranking semantics
- full page-model truth

For v1, this is a bounded structured subset only. If an upstream path emits
richer extraction content, we treat that as out of scope for this sample
rather than partially importing it and pretending the boundary stayed small.

## Map the checked-in valid artifact

```bash
python3 examples/stagehand-selector-scoped-extract-evidence/map_to_assay.py \
  examples/stagehand-selector-scoped-extract-evidence/fixtures/valid.stagehand.json \
  --output examples/stagehand-selector-scoped-extract-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-04-16T10:10:00Z \
  --overwrite
```

## Map the checked-in failure artifact

```bash
python3 examples/stagehand-selector-scoped-extract-evidence/map_to_assay.py \
  examples/stagehand-selector-scoped-extract-evidence/fixtures/failure.stagehand.json \
  --output examples/stagehand-selector-scoped-extract-evidence/fixtures/failure.assay.ndjson \
  --import-time 2026-04-16T10:15:00Z \
  --overwrite
```

## Check the malformed case

```bash
python3 examples/stagehand-selector-scoped-extract-evidence/map_to_assay.py \
  examples/stagehand-selector-scoped-extract-evidence/fixtures/malformed.stagehand.json \
  --output /tmp/stagehand-selector-malformed.assay.ndjson \
  --import-time 2026-04-16T10:20:00Z \
  --overwrite
```

This third command is expected to fail because the malformed fixture smuggles a
full `observe_actions` array into a lane that intentionally stays on one
observe-derived selector anchor only. That is an explicit product-boundary
rejection, not just parser hygiene:

- this lane is not the full `observe()` planning lane
- a free top-level action array would silently widen the claim surface
- if we need multi-action planning later, that should be a different lane or
  an explicitly narrowed future slice

The same rule applies to cardinality drift more broadly. For v1, any frozen
artifact containing multiple selectors, a full `observe()` action list, or
multiple result bags should be treated as malformed rather than partially
imported.

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- treat Stagehand DOM truth, snapshot truth, or execution truth as Assay truth
- imply that Assay independently verified runtime completeness
- claim that this sample already defines a stable upstream wire-format contract

This sample targets the smallest honest selector-scoped extraction artifact,
not Stagehand as a whole.

Additional caps in this sample:

- `selector_source` must stay `observe`
- `selector_kind` stays on `xpath`, `css`, or `other`
- `selector_ref` must stay short and non-empty
- `result` must stay a single small flat object
- `cache_status`, if present, stays on `HIT` or `MISS`
- optional refs such as `page_ref`, `run_ref`, and `metadata_ref` must stay
  opaque ids, not URLs

We are not asking Assay to inherit Stagehand page understanding, execution
semantics, or browser-state semantics as truth.

## Checked-in fixtures

- `fixtures/valid.stagehand.json`: bounded successful selector-scoped extract
  sample
- `fixtures/failure.stagehand.json`: bounded incomplete selector-scoped extract
  sample
- `fixtures/malformed.stagehand.json`: malformed import case that wrongly pulls
  a full `observe_actions` array into the lane
- `fixtures/valid.assay.ndjson`: mapped placeholder output with fixed import
  time
- `fixtures/failure.assay.ndjson`: mapped placeholder output with fixed import
  time
