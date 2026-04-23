# PLAN — P26 AgentEvals Trajectory Strict-Match Result Signal Evidence

- **Date:** 2026-04-23
- **Owner:** Evidence / External Interop
- **Status:** Planning lane
- **Scope (current repo state):** Explore one bounded AgentEvals-adjacent
  evidence lane built around a single deterministic trajectory strict-match
  result returned through the public AgentEvals evaluator API. This plan is
  for the smallest honest external-consumer seam only. It does not propose
  broad AgentEvals support, LangSmith evaluation-run support, graph-trajectory
  support, LLM-as-judge support, dataset support, or LangChain runtime truth.

## 1. Why this plan exists

`langchain-ai/agentevals` is a strong adjacent candidate because it publicly
positions itself around agent trajectory evaluation and exposes small
evaluator-return surfaces directly in its README.

That matters because Assay does not need "LangChain evals" as a platform.

It needs the smallest honest external-consumer seam that:

- already exists in named public docs
- is reviewable without importing full trajectory truth
- stays smaller than LangSmith runs or broader evaluator workflow semantics

The strongest first wedge is not a full eval run and not an LLM-judge result.

It is:

- one deterministic trajectory strict-match evaluator
- one returned result object
- one bounded result bag

The public strict-match example is the key reason to start here. The docs
show a direct returned object shaped like:

- `key`
- `score`
- `comment`

with `trajectory_strict_match` as the evaluator key in the strict-match path.

That is exactly the kind of small, named, returned signal Assay should prefer.

## 2. What this plan is and is not

This plan is for:

- one deterministic trajectory strict-match result
- one bounded result bag
- one discovery pass over the public evaluator call and returned object
- one small external-consumer artifact reduced from that returned result

This plan is not for:

- full AgentEvals support
- LLM-as-judge trajectory results
- graph trajectory evaluators
- LangSmith experiment or `evaluate(...)` result wrappers
- dataset truth
- raw `outputs` or `reference_outputs` truth
- evaluator prompt or model-config truth
- LangChain or LangGraph runtime truth

## 3. Hard positioning rule

P26 v1 claims only one bounded AgentEvals trajectory strict-match result as
imported external evaluation signal evidence. It does not claim trajectory
truth, reference truth, evaluator prompt truth, LangSmith truth, dataset
truth, or LangChain runtime truth.

That means:

- AgentEvals remains the source of the observed result
- Assay imports only the smallest honest returned result surface
- Assay does not inherit broader eval-run semantics as truth

## 4. Recommended seam

The first seam should stay on exactly one move:

- call the public deterministic trajectory match path through
  `create_trajectory_match_evaluator(trajectory_match_mode="strict")` or
  `createTrajectoryMatchEvaluator({ trajectoryMatchMode: "strict" })`
- reduce exactly one returned result object

Not:

- `create_trajectory_llm_as_judge(...)`
- graph trajectory evaluators
- LangSmith `evaluate(...)` envelopes
- dataset-backed experiment rows
- full trajectory payload export

This is intentionally smaller than the broader AgentEvals surface.

The strict-match path is the best first seam because it is:

- deterministic
- public in the README
- already shown as returning a small object
- smaller than the LLM-as-judge path, which already widens into prompt,
  model, and free-text reasoning semantics

## 5. Canonical v1 artifact thesis

The reduced artifact should stay on a single returned strict-match result.

The v1 artifact must be frozen from a captured returned evaluator object, not
from README examples or caller-side expectations. The public docs are enough to
justify the lane, but the raw returned result is the source of truth for fixture
freeze.

Illustrative v1 shape:

```json
{
  "schema": "agentevals.trajectory-strict-match.export.v1",
  "framework": "agentevals",
  "surface": "trajectory_strict_match_result",
  "target_kind": "trajectory",
  "evaluator_key": "trajectory_strict_match",
  "result": {
    "score": false
  }
}
```

Optional reviewer support, only if naturally present on the returned result:

- `result.comment`

Not allowed in v1:

- raw `outputs`
- raw `reference_outputs`
- LangSmith run or experiment wrappers
- dataset identifiers
- prompt or model metadata
- evaluator configuration blobs
- synthetic timestamps
- synthetic trajectory identifiers

## 6. Field boundaries

### 6.1 `target_kind`

For v1, the only allowed value is:

- `trajectory`

This keeps the lane on one trajectory-evaluation result rather than wider
session, thread, or graph semantics.

`target_kind = "trajectory"` names the evaluation level only. It does not imply
that v1 carries a stable target identity.

### 6.2 No `target_id_ref` in v1

The returned strict-match result does not naturally carry a stable target
identifier.

Therefore v1 should not invent one.

Assay must not synthesize a target reference from:

- caller-side harness state
- dataset row identity
- LangSmith wrappers
- hashes of full trajectories
- internal run bookkeeping

If a future public returned result naturally carries a stable trajectory
anchor, the lane can be revisited. V1 should stay honest and omit it.

### 6.3 `evaluator_key`

This is the canonical Assay-side name for the returned AgentEvals `key`.

It should stay:

- required
- short
- observed
- reviewer-readable

It must not become:

- a taxonomy import
- evaluator configuration truth
- a broader LangChain evaluation ontology

For the strict-match-first lane, the expected v1 key is:

- `trajectory_strict_match`

### 6.4 `result.score`

This is the core bounded evaluation signal.

For v1 strict-match, it should remain:

- required
- boolean
- observed exactly as returned

It must not be treated as:

- universal evaluator truth
- ranking truth
- normalized cross-evaluator semantics

### 6.5 `result.comment`

This is optional reviewer support only.

It must remain:

- optional
- bounded
- short when present

`comment` is never required. If present, it may be omitted during reduction if
it is too long, too rich, multiline, structured, or otherwise broader than the
small returned-result evidence surface.

It must not become:

- chain-of-thought import
- raw reasoning transcript
- prompt or rubric payload
- embedded trajectory content dump
- structured reasoning blob

Empty or whitespace-only comments should be omitted or treated as malformed.

## 7. Observed vs derived rule

P26 v1 should remain almost entirely observed.

Observed:

- returned `key`
- returned `score`
- returned `comment` when naturally present and non-empty

Derived:

- renaming returned `key` into canonical `evaluator_key`
- minimal field normalization required to freeze the artifact

The plan must not derive:

- timestamps
- trajectory identifiers
- dataset or run lineage
- evaluator-mode truth beyond what is already explicit in the returned key

Evaluator inputs are discovery material only:

- `outputs` may be captured for discovery only
- `reference_outputs` may be captured for discovery only
- raw trajectory payloads must never enter the canonical v1 artifact
- their only role is to prove that the returned result is genuinely smaller
  than the evaluated payloads

## 8. Cardinality rule

This lane is for exactly one returned evaluation result object.

Therefore v1 artifacts should be malformed if they contain:

- multiple evaluation results
- result arrays
- batch evaluator wrappers
- LangSmith experiment result envelopes
- dataset row bundles
- full trajectory-plus-result payloads
- evaluator configuration fields beyond the returned key
- trajectory match mode fields
- model or prompt metadata

No partial import of larger evaluation bundles should be allowed in v1.

V1 must fail closed on larger evaluation, dataset, or experiment wrappers
rather than partially importing the "first relevant" result.

## 9. Discovery gate

P26 should not advance on docs snippets alone. Freeze nothing until one raw
strict-match return object is captured from the public evaluator call and stored
separately from all caller inputs.

Required first proof:

- call one real strict-match evaluator through the public AgentEvals API
- capture raw input `outputs` and `reference_outputs` separately as discovery
  artifacts
- capture the raw returned result object as its own discovery artifact
- compare the input boundary to the returned-result boundary before freezing
  any reduced artifact

Keep raw inputs and raw returned result separate. Do not treat the evaluator
inputs as part of the returned public result shape.

If the observed returned shape differs materially across Python and TypeScript,
the lane should freeze per language first rather than pretending there is a
single cross-language v1 artifact by default.

## 10. Initial malformed rules

Artifacts should be malformed if they contain:

- no `evaluator_key`
- no `result`
- a non-boolean `result.score`
- empty or whitespace-only `result.comment`
- raw trajectory payloads
- raw reference trajectory payloads
- dataset or experiment identifiers
- LangSmith wrapper fields
- evaluator configuration fields
- trajectory match mode fields
- prompt, model, or rubric metadata
- arrays of evaluation results
- partial imports from larger LangSmith or LangChain evaluation wrappers

## 11. Repository deliverables for first execution

If discovery validates the seam, the first concrete P26 lane should include:

- a formal example directory
- one live discovery note with input vs returned field presence
- one small mapper
- valid, failure, and malformed fixtures
- generated placeholder NDJSON outputs for valid cases

Suggested layout:

```text
examples/
  agentevals-trajectory-strict-match-evidence/
    README.md
    map_to_assay.py
    capture_probe.py
    discovery/
      FIELD_PRESENCE.md
    fixtures/
      valid.agentevals.json
      failure.agentevals.json
      malformed.agentevals.json
      valid.assay.ndjson
      failure.assay.ndjson
```

## 12. Success criteria

This plan succeeds when:

- Assay has one credible AgentEvals-adjacent seam that is smaller than
  AgentEvals or LangSmith evaluation truth
- the lane stays on a single returned strict-match result
- the reduced artifact remains smaller than trajectory payloads or eval-run
  wrappers
- discovery proves the returned shape before any contract freeze

## 13. Final judgment

P26 should be a strict-match-first AgentEvals lane: one returned deterministic
trajectory match result, and nothing broader.
