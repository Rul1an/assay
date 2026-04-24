# PLAN — P28 Promptfoo Assertion GradingResult Evidence

- **Date:** 2026-04-24
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Explore one bounded Promptfoo-adjacent
  evidence lane built around a single deterministic assertion `GradingResult`
  surfaced through Promptfoo's public assertion/eval output path. This plan is
  for the smallest honest external-consumer surface only. It does not propose
  broad Promptfoo support, red-team result support, prompt comparison support,
  provider output import, dataset import, trace import, or Promptfoo platform
  truth.

## 1. Why this plan exists

`promptfoo/promptfoo` is a strong adjacent P28 candidate because it is not an
agent runtime or tracing platform. It sits in the eval-as-CI space: declarative
tests, assertions, JSON/JSONL export, and CI-friendly pass/fail outputs.

That makes it adjacent to Assay in a slightly different way from the recent
span, evaluator, and returned-score lanes.

The risk is also obvious: Promptfoo can easily become a whole eval-run import.

P28 should not do that.

The strongest first Promptfoo wedge is not:

- a full `promptfoo eval` results export
- a red-team scan report
- a prompt/provider comparison matrix
- raw model outputs
- assertion configuration truth
- Promptfoo's web viewer or platform state

It is:

- one deterministic assertion
- one surfaced `GradingResult`
- one bounded pass/score/reason result bag

The public Promptfoo docs make this seam plausible. Assertion functions can
return a `GradingResult`, deterministic assertions such as `equals` are
documented, and export docs show JSON/JSONL result surfaces with pass/fail and
score information. That is enough to justify discovery.

It is not enough to freeze a contract before capture.

## 2. What this plan is and is not

This plan is for:

- one deterministic Promptfoo assertion result
- one bounded `GradingResult`-shaped result bag
- one discovery pass over the public surfaced result shape
- one small external-consumer artifact reduced from that surfaced result

This plan is not for:

- full Promptfoo support
- red-team or vulnerability scan reports
- prompt comparison truth
- provider output truth
- raw prompt, vars, expected, or output payload truth
- Promptfoo config truth
- dataset, eval-run, or stats truth
- model-graded assertion semantics
- token, cost, latency, or provider telemetry
- web viewer, cloud, or sharing semantics

## 3. Hard positioning rule

P28 v1 claims only one bounded Promptfoo deterministic assertion
`GradingResult` as imported external evaluation signal evidence. It does not
claim output truth, expected-answer truth, prompt truth, provider truth,
Promptfoo config truth, red-team truth, dataset truth, or eval-run truth.

That means:

- Promptfoo remains the source of the observed assertion result
- Assay imports only the smallest honest surfaced result shape
- Assay does not inherit broader eval-run semantics as truth

## 4. Recommended surface

The first surface should stay on exactly one move:

- run one public deterministic Promptfoo assertion, preferably `equals`
- capture the surfaced assertion `GradingResult` from the public CLI output or
  public Node package result path
- reduce exactly one assertion result object

Not:

- `llm-rubric`
- model-graded assertions
- red-team plugins
- full JSON output envelopes
- JSONL output lines as a whole
- provider response bodies
- prompt matrix rows
- stats summaries
- config exports

This is intentionally smaller than the broader Promptfoo surface.

The deterministic `equals` path is the best first Promptfoo surface because it
is:

- public in the deterministic assertion docs
- independent of model-graded rubrics
- small enough to validate without importing raw prompt/output payloads
- close to the pass/fail CI shape users already expect from Promptfoo

## 5. Canonical v1 artifact thesis

The reduced artifact should stay on a single surfaced deterministic assertion
result.

The v1 artifact must be frozen from a captured surfaced Promptfoo assertion
result object, not from docs snippets, TypeScript interface snippets, or
caller-side expectations. Public docs justify the lane, but the raw surfaced
result is the source of truth for fixture freeze.

Illustrative v1 shape:

```json
{
  "schema": "promptfoo.assertion-grading-result.export.v1",
  "framework": "promptfoo",
  "surface": "assertion_grading_result",
  "target_kind": "promptfoo_output_assertion",
  "assertion_type": "equals",
  "result": {
    "pass": true,
    "score": 1,
    "reason": "Assertion passed"
  }
}
```

Optional reviewer support, only if naturally present on the surfaced assertion
result:

- `result.reason`

Not allowed in v1:

- raw `prompt`
- raw `output`
- raw `expected`
- raw `vars`
- raw assertion config
- Promptfoo full JSON/YAML/XML export envelopes
- JSONL output lines as canonical artifacts
- provider identifiers or response bodies
- token, cost, latency, or stats objects
- `componentResults`
- `namedScores`
- `tokensUsed`
- synthetic timestamps
- synthetic prompt, output, expected, or test identifiers

## 6. Field boundaries

### 6.1 `target_kind`

For v1, the only allowed value is:

- `promptfoo_output_assertion`

This names the evaluation level. It does not imply that v1 carries stable
prompt identity, output identity, expected-answer identity, provider identity,
test-case identity, or run identity.

### 6.2 No `target_id_ref` in v1

A single surfaced `GradingResult` does not naturally guarantee a stable target
identifier.

Therefore v1 should not invent one.

Assay must not synthesize a target reference from:

- Promptfoo `testIdx` or `promptIdx`
- provider IDs
- prompt text
- vars
- hashes of raw outputs or expected values
- eval-run IDs
- JSONL line positions
- internal run bookkeeping

If a future public surfaced assertion result naturally carries a stable
assertion anchor, the lane can be revisited. V1 should stay honest and omit it.

### 6.3 `assertion_type`

This is the canonical Assay-side name for the observed Promptfoo assertion.

It should stay:

- required
- short
- observed from the surfaced assertion result or adjacent public assertion
  descriptor
- reviewer-readable

It must not become:

- Promptfoo assertion taxonomy truth
- assertion configuration truth
- a broader Promptfoo eval ontology

For the first lane, the expected v1 value is:

- `equals`

Discovery must confirm whether the surfaced result naturally carries the
assertion type. If it does not, the reducer may use the explicitly invoked
assertion type, but must document that as a minimal reduction choice rather
than returned-result truth.

### 6.4 `result.pass`

This is the core bounded assertion outcome.

For v1 deterministic assertion evidence, it should remain:

- required
- boolean
- observed exactly as surfaced

It must not be treated as:

- universal correctness truth
- proof that the model output is true
- proof that the expected value is correct
- Promptfoo run success as a whole

### 6.5 `result.score`

This is the numeric score attached to the assertion result.

For first execution, it should remain:

- required
- numeric
- observed exactly as surfaced
- bounded to the shape proven by discovery

For a deterministic `equals` assertion, discovery should decide whether the
first reducer accepts only `0` and `1` or preserves a wider numeric shape if
Promptfoo surfaces one naturally.

The plan should not widen `result.score` to generic scorer semantics before
capture.

### 6.6 `result.reason`

This is optional reviewer support only.

It must remain:

- optional
- short
- bounded
- non-empty when present
- derived only from the surfaced assertion result

It must not become:

- chain-of-thought
- prompt or output transcript
- provider error dump
- model-graded rubric explanation
- multi-line structured reasoning blob

The reducer may omit `reason` even when present if it is too long or too rich.

## 7. Observed vs derived rule

P28 v1 should remain almost entirely observed.

Observed:

- surfaced assertion type, if naturally present
- surfaced `pass`
- surfaced `score`
- surfaced `reason`, if short and naturally present

Derived:

- fixed `framework = "promptfoo"`
- fixed `surface = "assertion_grading_result"`
- fixed `target_kind = "promptfoo_output_assertion"`
- using the explicitly invoked assertion type only if the surfaced result does
  not naturally carry one

The plan must not derive:

- timestamps
- target identifiers
- run identifiers
- prompt, provider, dataset, or config lineage
- output/expected hashes as identity
- pass/fail summaries for the run

Promptfoo inputs and wrappers are discovery material only:

- prompt text may be captured for discovery only
- output may be captured for discovery only
- expected value may be captured for discovery only
- vars may be captured for discovery only
- assertion config may be captured for discovery only
- full JSON/JSONL export wrappers may be captured only to locate the surfaced
  assertion result

None of those fields may enter the canonical v1 artifact.

## 8. Cardinality rule

This lane is for exactly one surfaced deterministic assertion result.

Therefore v1 artifacts should be malformed if they contain:

- multiple assertion results
- arrays of results
- `componentResults`
- `namedScores`
- full JSON/JSONL/YAML/XML export envelopes
- prompt/provider/test matrices
- stats summaries
- red-team result bundles
- model-graded rubric outputs
- provider outputs or response bodies
- raw prompt, vars, expected, or output values
- assertion configuration objects

No partial import of larger Promptfoo eval results should be allowed in v1.

V1 must fail closed on larger eval/export wrappers rather than partially
importing the "first relevant" assertion result.

## 9. Discovery gate

P28 should not advance on docs snippets alone. Freeze nothing until one raw
surfaced deterministic Promptfoo assertion result is captured from a public
Promptfoo path and stored separately from all emitted inputs and wrappers.

Required first proof:

- run one real deterministic `equals` assertion through Promptfoo
- capture the raw prompt/output/expected/config inputs separately as discovery
  artifacts
- capture the public surfaced assertion result separately
- confirm whether the result came from CLI JSON output, JSONL output, or the
  Node package result path
- compare emitted inputs, export wrappers, and the surfaced assertion result
  before freezing any reduced artifact

Keep these separate:

- emitted Promptfoo config and assertion input
- provider/model output
- full Promptfoo export envelope
- extracted surfaced `GradingResult`
- reduced Assay-facing artifact

Do not treat full Promptfoo JSON output as equivalent to the assertion result
shape. Promptfoo JSON/YAML/XML exports can include config and redacted
environment data, so importing them as v1 evidence would be too broad.

If CLI JSON, JSONL, and Node package paths return materially different shapes,
the lane should freeze one surfaced path first rather than pretending there is a
single cross-output Promptfoo v1 artifact by default.

## 10. Initial malformed rules

Artifacts should be malformed if they contain:

- no `assertion_type`
- no `result`
- no `result.pass`
- no `result.score`
- non-boolean `result.pass`
- non-numeric `result.score`
- empty or whitespace-only `result.reason`
- raw prompt
- raw output
- raw expected
- raw vars
- assertion config
- provider IDs or response bodies
- full Promptfoo export wrappers
- JSONL line wrappers
- stats, latency, cost, or token usage
- `componentResults`
- `namedScores`
- red-team or model-graded assertion metadata
- arrays of assertion results
- partial imports from larger Promptfoo eval results

## 11. Repository deliverables for first execution

If discovery validates the surface, the first concrete P28 lane should include:

- a formal example directory
- one live discovery note with emitted vs surfaced field presence
- one small mapper
- valid, failure, and malformed fixtures
- generated placeholder NDJSON outputs for valid cases

Suggested layout:

```text
examples/
  promptfoo-assertion-grading-result-evidence/
    README.md
    map_to_assay.py
    capture_probe.mjs
    discovery/
      FIELD_PRESENCE.md
    fixtures/
      valid.promptfoo.json
      failure.promptfoo.json
      malformed.promptfoo.json
      valid.assay.ndjson
      failure.assay.ndjson
```

## 12. Outward strategy

Promptfoo has issues enabled and discussions disabled. The repo norm is mostly
direct technical issues with concrete examples.

P28 should not open with a broad integration ask.

If the sample lands, outreach should be a compact issue that asks one narrow
question:

> Is the surfaced deterministic assertion `GradingResult` the right minimal
> public result boundary for external evidence consumers, or should consumers
> anchor to a different JSON/Node result surface?

Keep the tone warm, concise, and concrete. Do not ask Promptfoo maintainers to
validate Assay's broader evidence model.

## 13. Success criteria

This plan succeeds when:

- Assay has one credible eval-as-CI adjacent surface that is smaller than
  Promptfoo eval-run truth
- the lane stays on a single deterministic assertion result
- the reduced artifact remains smaller than prompt/output/expected/config
  payloads
- discovery proves the surfaced shape before any contract freeze
- malformed rules prevent wrapper drift into Promptfoo platform or run truth

## 14. Final judgment

P28 should be a Promptfoo deterministic assertion `GradingResult` lane: one
surfaced pass/score/reason result, and nothing broader.
