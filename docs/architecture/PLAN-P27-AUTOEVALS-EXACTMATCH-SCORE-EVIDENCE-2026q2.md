# PLAN — P27 AutoEvals ExactMatch Score Evidence

- **Date:** 2026-04-24
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Explore one bounded AutoEvals-adjacent
  evidence lane built around a single deterministic `ExactMatch` score object
  returned through the public AutoEvals scorer API. This plan is for the
  smallest honest external-consumer surface only. It does not propose broad
  AutoEvals support, Braintrust experiment logging support, LLM judge support,
  RAG scorer support, JSON/list scorer support, or model-provider truth.

## 1. Why this plan exists

`braintrustdata/autoevals` is a strong non-LangChain P27 candidate because it is
an active evaluator package with public Python and TypeScript surfaces, open
issues, and a product boundary adjacent to Braintrust without requiring
Braintrust logging.

That matters because the recent lanes have leaned into LangChain-adjacent
evaluation surfaces. P27 should test the same small-result discipline in a
different evaluation community.

The strongest first AutoEvals wedge is not:

- Factuality or other LLM-as-a-judge scorers
- RAGAS-style RAG scorers
- JSON or list aggregate scorers
- Braintrust `Eval(...)` / experiment logging
- custom scorer authoring as a whole

It is:

- one deterministic `ExactMatch` scorer
- one returned score object
- one bounded result bag

The public scorer reference describes `ExactMatch` as a binary scorer that
checks exact equality, with a score range of `0` or `1`. The AutoEvals README
also shows the core custom scorer return shape as a `Score` with:

- `name`
- `score`
- optional metadata in broader scorer families

That is the kind of small returned result Assay should prefer over richer
judge, retrieval, JSON, list, or experiment surfaces.

## 2. What this plan is and is not

This plan is for:

- one deterministic `ExactMatch` score object
- one bounded result bag
- one discovery pass over the public scorer call and returned object
- one small external-consumer artifact reduced from that returned score

This plan is not for:

- full AutoEvals support
- LLM judge scorer results
- Braintrust experiment or logging wrappers
- RAG scorer contexts
- JSON scorer result trees
- list scorer bundles
- custom scorer framework support
- model-provider or prompt truth
- raw `output`, `expected`, or `input` truth

## 3. Hard positioning rule

P27 v1 claims only one bounded AutoEvals `ExactMatch` score object as imported
external evaluation signal evidence. It does not claim output truth, expected
answer truth, Braintrust truth, dataset truth, scorer-family truth, model truth,
or prompt truth.

That means:

- AutoEvals remains the source of the observed score
- Assay imports only the smallest honest returned score surface
- Assay does not inherit broader experiment or logging semantics as truth

## 4. Recommended surface

The first surface should stay on exactly one move:

- call the public deterministic `ExactMatch` scorer through the Python or
  TypeScript AutoEvals API
- reduce exactly one returned score object

Not:

- `Factuality`
- `ClosedQA`
- `Summary`
- RAG scorers
- JSON scorers
- list scorers
- Braintrust `Eval(...)`
- model-provider-backed scorers
- raw output/expected payload export

This is intentionally smaller than the broader AutoEvals surface.

The `ExactMatch` path is the best first AutoEvals surface because it is:

- deterministic
- public in the scorer reference
- independent of LLM clients, prompts, model configuration, and Braintrust
  experiment logging
- small enough to review without importing the compared payloads

## 5. Canonical v1 artifact thesis

The reduced artifact should stay on a single returned `ExactMatch` score object.

The v1 artifact must be frozen only from a captured returned `ExactMatch` score
object, not from README examples or caller-side expectations. The public docs
are enough to justify the lane, but the raw returned score object is the only
source of truth for fixture freeze.

Illustrative v1 shape:

```json
{
  "schema": "autoevals.exactmatch-score.export.v1",
  "framework": "autoevals",
  "surface": "exactmatch_score",
  "target_kind": "output_expected_pair",
  "scorer_name": "ExactMatch",
  "result": {
    "score": 1
  }
}
```

Optional reviewer support is out of scope by default and may be added only if
discovery proves it is naturally present on the returned `ExactMatch` score
object:

- `result.metadata_ref`

Not allowed in v1:

- raw `output`
- raw `expected`
- raw `input`
- inline metadata bags
- Braintrust experiment or span wrappers
- scorer configuration blobs
- prompt, model, rubric, context, or provider metadata
- synthetic timestamps
- synthetic output or expected identifiers

## 6. Field boundaries

### 6.1 `target_kind`

For v1, the only allowed value is:

- `output_expected_pair`

This names the comparison level. It does not imply that v1 carries stable
output identity, expected-answer identity, dataset row identity, or run identity.

### 6.2 No `target_id_ref` in v1

The returned `ExactMatch` score object does not naturally carry a stable target
identifier.

Therefore v1 should not invent one.

Assay must not synthesize a target reference from:

- caller-side harness state
- dataset row identity
- hashes of raw outputs or expected values
- Braintrust wrappers
- internal run bookkeeping

If a future public returned result naturally carries a stable comparison anchor,
the lane can be revisited. V1 should stay honest and omit it.

### 6.3 `scorer_name`

This is the canonical Assay-side name for the observed AutoEvals scorer.

It should stay:

- required
- short
- observed when the returned score object naturally carries a name field
- reviewer-readable

It must not become:

- a taxonomy import
- scorer configuration truth
- a broader AutoEvals scorer ontology

For the first lane, the expected scorer identity is:

- `ExactMatch`

Discovery must confirm the actual returned field names before fixture freeze.
`scorer_name` should preserve the observed returned scorer name when present.
If the returned score object does not naturally carry a name field, the reducer
may use the explicitly invoked scorer identity, but must document that as a
minimal reduction choice rather than returned-result truth.

### 6.4 `result.score`

This is the core bounded evaluation signal.

For v1 `ExactMatch`, it should remain:

- required
- exactly `0` or `1`
- observed exactly as returned

V1 accepts only the exact returned binary score shape proven by discovery. Do
not widen to generic numeric semantics unless the public returned object
requires it.

That means the reducer should not accept booleans masquerading as numbers, or
float variants such as `1.0`, unless discovery proves that is the exact
returned public shape for the selected language surface.

It must not be treated as:

- universal evaluator truth
- ranking truth
- normalized cross-scorer semantics
- proof that either compared value is correct

The score only reports AutoEvals' exact comparison result for the supplied
output/expected pair.

### 6.5 `result.metadata_ref`

Metadata is out of scope by default for `ExactMatch` v1.

No `metadata_ref` should appear unless discovery shows a naturally returned
metadata field on the `ExactMatch` public result surface and proves a tiny
stable subset is genuinely necessary for review. If that happens, P27 should
prefer:

- `metadata_ref`

over importing the raw metadata object.

For first execution, raw inline metadata should be malformed. This keeps
LLM-judge rationales, RAG context, provider payloads, and Braintrust logging
details out of the first lane.

## 7. Observed vs derived rule

P27 v1 should remain almost entirely observed.

Observed:

- returned scorer name or equivalent score name
- returned `score`
- returned metadata presence only as discovery information

Derived:

- renaming an observed score name into canonical `scorer_name`
- minimal field normalization required to freeze the artifact
- fixed `target_kind = "output_expected_pair"` to name the comparison level

The plan must not derive:

- timestamps
- target identifiers
- dataset or run lineage
- scorer-family truth
- output/expected hashes as identity

Scorer inputs are discovery material only:

- `output` may be captured for discovery only
- `expected` may be captured for discovery only
- `input` may be captured for discovery only if the public call requires or
  naturally accepts it
- raw compared payloads must never enter the canonical v1 artifact
- raw compared payloads must not be reduced into synthetic identifiers or
  hashes for v1
- their only role is to prove that the returned score is genuinely smaller than
  the evaluated payloads

## 8. Cardinality rule

This lane is for exactly one returned score object.

Therefore v1 artifacts should be malformed if they contain:

- multiple score objects
- score arrays
- JSON scorer result trees
- list scorer bundles
- Braintrust experiment wrappers
- dataset row bundles
- full output/expected-plus-score payloads
- scorer configuration fields
- model, prompt, rubric, provider, or context metadata

No partial import of larger evaluation bundles should be allowed in v1.

V1 must fail closed on larger scorer, dataset, or experiment wrappers rather
than partially importing the "first relevant" score.

## 9. Discovery gate

P27 should not advance on docs snippets alone. Freeze nothing until one raw
`ExactMatch` return object is captured from the public scorer call and stored
separately from all caller inputs.

Required first proof:

- call one real `ExactMatch` scorer through the public AutoEvals API
- capture raw `output` and `expected` separately as discovery artifacts
- keep those raw compared values only as discovery artifacts
- capture the raw returned score object as its own discovery artifact
- compare the input boundary to the returned-score boundary before freezing
  any reduced artifact

Keep raw inputs and raw returned score separate. Do not treat scorer inputs as
part of the returned public result shape. Do not let `output`, `expected`, or
optional `input` enter the canonical v1 artifact, even as redacted values,
synthetic identifiers, or hashes.

If Python and TypeScript return materially different score shapes, the lane
should freeze per language first rather than pretending there is a single
cross-language v1 artifact by default. Do not freeze a cross-language artifact
contract until discovery proves the returned shape is materially compatible.

## 10. Initial malformed rules

Artifacts should be malformed if they contain:

- no `scorer_name`
- no `result`
- a `result.score` shape other than the discovered binary score shape
- a `result.score` other than `0` or `1`
- raw output
- raw expected
- raw input
- inline metadata bags
- dataset or experiment identifiers
- Braintrust wrapper fields
- scorer configuration fields
- prompt, model, rubric, provider, or context metadata
- JSON/list scorer aggregate outputs
- arrays of score objects
- partial imports from larger Braintrust or AutoEvals wrappers

## 11. Repository deliverables for first execution

If discovery validates the surface, the first concrete P27 lane should include:

- a formal example directory
- one live discovery note with input vs returned field presence
- one small mapper
- valid, failure, and malformed fixtures
- generated placeholder NDJSON outputs for valid cases

Suggested layout:

```text
examples/
  autoevals-exactmatch-evidence/
    README.md
    map_to_assay.py
    capture_probe.py
    discovery/
      FIELD_PRESENCE.md
    fixtures/
      valid.autoevals.json
      failure.autoevals.json
      malformed.autoevals.json
      valid.assay.ndjson
      failure.assay.ndjson
```

## 12. Success criteria

This plan succeeds when:

- Assay has one credible non-LangChain evaluator surface that is smaller than
  AutoEvals or Braintrust evaluation truth
- the lane stays on a single returned `ExactMatch` score object
- the reduced artifact remains smaller than output/expected payloads or
  experiment wrappers
- discovery proves the returned shape before any contract freeze

## 13. Final judgment

P27 should be an AutoEvals `ExactMatch` lane: one returned deterministic
output/expected comparison score, and nothing broader.
