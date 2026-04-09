# PLAN — P14 Mastra Scorer / Experiment-Result Evidence Interop (2026 Q2)

- **Date:** 2026-04-09
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next Mastra interop lane after the current
  Browser Use, Visa TAP, and Langfuse planning wave. Include a small sample
  implementation, with no outward issue and no contract freeze in this slice.

## 1. Why this plan exists

After the current wave, the next lane should still pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream semantics as
   truth,
3. the repo has at least one natural maintainer channel for one small
   sample-backed boundary question.

`mastra-ai/mastra` now fits that pattern well enough to justify a formal plan:

- the repo is large, active, and visibly current
- the official product story now treats **scorers**, **datasets**, and
  **experiments** as first-class reliability surfaces
- the docs explicitly mark the older evals API as legacy, which helps narrow
  the seam to the current scorer-centered direction
- the repo exposes issues, even though Discussions are not enabled

That makes Mastra a strong next candidate, but only if Assay starts from a
small scorer / experiment-result artifact rather than another tracing or
dashboard-shaped lane.

This is **not** a tracing export plan.

This is **not** a Studio metrics export plan.

This is **not** a dashboard synchronization plan.

This is **not** a legacy `evaluate()` plan.

This is a plan for a **bounded scorer-result / experiment-item seam derived
from the current Mastra reliability stack**.

## 2. Why Mastra is a good `P14` candidate

Mastra sits in a useful position in the current queue:

- newer and faster-moving than many older framework candidates
- strong enough to matter strategically
- adjacent to evaluation, but not identical to Agno or Langfuse
- socially cleaner than a pure platform-on-platform trace pitch

At the same time, the channel shape is weaker than Agno or Browser Use:

- no GitHub Discussions
- outward route is issue-first

That means `P14` is a good **next planned build lane** and a good fallback when
Langfuse feels too platform-adjacent, but it should still open with a narrower,
sample-backed technical issue rather than a broad pitch.

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest Mastra scorer / experiment-result
> surface, not a trace export, observability export, dashboard export, or
> runtime truth surface.

That means:

- Mastra is the upstream framework context, not the truth source
- scorer outputs are bounded evaluation artifacts, not system truth
- dataset and experiment context are reviewability aids, not truth semantics
- Assay stays an external evidence consumer, not a scorer, dashboard, or trace
  authority

## 4. Why not tracing-first

Mastra now has a substantial observability story:

- logs
- traces
- Studio metrics
- experiment comparison in Studio

That is real, but it is still the wrong first wedge.

Why:

- it would look too similar to earlier trace and telemetry lanes
- it would blur product observability semantics into evidence semantics
- it would miss the narrower reliability surface Mastra now documents through
  scorers and experiments
- it would underuse the fact that Mastra itself is moving away from “evals” as
  a generic label and toward a scorer pipeline with explicit output shaping

The cleaner first wedge is:

- one artifact derived from a scorer result
- one bounded experiment-item context
- one bounded dataset version reference
- optional short reason text only if the chosen sample shape naturally carries
  it

This keeps `P14` reviewable, current, and clearly distinct from trace-first
interop.

## 5. Why not legacy evals-first

Mastra still documents the older evals API, but the docs now explicitly label
it as legacy and point readers to the newer scorer system.

That matters for the plan.

The first seam should not be:

- legacy `evals` wiring
- old metric objects as the primary artifact
- CI screenshots or dashboard screenshots

The first seam should instead align with Mastra’s current direction:

- scorer-first
- experiment-aware
- dataset-version-aware
- bounded result artifact

That keeps the lane honest relative to upstream’s actual 2026 trajectory.

## 6. Why scorer-result-first, not reason-first

Mastra’s current scorer architecture is especially relevant because it
separates:

- preprocessing / analysis
- score generation
- optional reason generation

That is a good fit for Assay.

The safest first wedge is the **score-bearing result artifact**, not the
explanatory text.

Why:

- numeric or categorical scorer output is easier to keep bounded
- explanation text becomes semantically slippery very quickly
- recent LLM-as-a-judge research continues to show that judge explanations are
  useful for debugging but weaker as portable truth surfaces than bounded,
  calibrated result artifacts
- Mastra’s own scorer framing emphasizes deterministic score generation as the
  stable output layer

So in v1:

- `score` is first-class
- `scorer_reason_ref` is optional
- no evaluator prompt
- no reasoning transcript
- no long free-text explanation channel

## 7. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented Mastra scorer
/ experiment-result path** as the first external-consumer seam.

The intended upstream anchors are:

- scorer output
- dataset-backed experiment execution
- bounded experiment item result context
- optional CI execution only if it stays small and deterministic

The first artifact should stay at the **single item result** level, not the
full experiment summary and not the full observability tree.

The first artifact should therefore center on:

- one experiment name
- one dataset reference
- one bounded dataset version reference
- one item reference
- one scorer name
- one bounded score value
- one short outcome label
- optional short reason reference only if already present

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented
> Mastra scorer / experiment-result surface, not a claim that Mastra already
> guarantees one fixed wire-export contract for external evidence consumers.

## 8. v1 artifact contract

### 8.1 Required fields

The first sample should require:

- `schema`
- `framework`
- `surface`
- `experiment_name`
- `dataset_ref`
- `dataset_version_ref`
- `item_ref`
- `target_type`
- `scorer_name`
- `score`
- `timestamp`
- `outcome`

These required fields belong to the frozen sample artifact shape.

They must be described as:

- sample-level reductions derived from documented Mastra scorer and experiment
  surfaces
- not an upstream guarantee that Mastra already ships one canonical serialized
  export object with these exact field names

### 8.2 Optional fields

The first sample may include:

- `scorer_reason_ref`
- `run_ref`
- `target_ref`
- `error_label`
- `scorer_type`

### 8.3 Important field boundaries

#### `dataset_version_ref`

This field is required because dataset versioning is part of Mastra’s current
dataset / experiment story.

In v1, it must stay a bounded version pointer only:

- integer version
- short version label
- short normalized version reference

Not allowed in v1:

- full dataset export
- full dataset item body replay
- dataset schema dump

This field belongs to the frozen sample shape, not to an upstream claim that
Mastra guarantees one universal serialized dataset-version contract for
external consumers.

#### `item_ref`

This field is required and must stay an opaque dataset-item or experiment-item
reference only.

Allowed:

- item id
- short item label
- short opaque reference

Not allowed in v1:

- full input body as the primary seam
- full expected-output body as the primary seam
- rich trace linkage

#### `target_type`

This field is required because Mastra scorers can attach to different runtime
contexts such as an agent response or workflow step.

In v1, keep it small:

- `agent`
- `workflow_step`

It must remain a bounded sample-level label, not a claim that Assay validated
the whole runtime context independently.

#### `score`

This field is required in the frozen sample shape.

It should stay small and bounded:

- numeric score
- or short categorical score if the chosen sample shape truly requires it

Not allowed in v1:

- unbounded metric blobs
- full distribution dumps
- token-usage side channels
- observability rollups

This field belongs to the sample shape, not to an upstream claim that Mastra
guarantees one universal serialized score contract.

#### `outcome`

This field is required in the frozen sample shape, but it remains sample-level
only.

It should be a short bounded interpretation such as:

- `scored`
- `failed`

It must not become:

- quality truth
- policy truth
- runtime correctness truth

#### `scorer_reason_ref`

This field is optional in v1.

If present, it must remain extremely small:

- short label
- short extracted reason
- opaque reason reference

Not allowed in v1:

- full evaluator explanation transcript
- scorer prompt
- chain-of-thought style payload
- compliance narrative

This field must remain a debugging or review aid, not a second richer evidence
channel.

#### Optional references

The optional reference fields must stay bounded:

- short label
- opaque id
- short normalized reference

Not allowed in v1:

- full trace trees
- full logs
- screenshots
- Studio exports
- metrics dashboards

## 9. Assay-side meaning

The sample may only claim bounded scorer-result observation.

Assay must not treat as truth:

- runtime correctness
- workflow correctness
- trace correctness
- scorer correctness
- model-quality truth beyond the observed artifact
- dashboard truth
- observability truth

Common anti-overclaim sentence:

> We are not asking Assay to inherit Mastra scorer semantics, experiment
> semantics, dashboard semantics, or observability semantics as truth.

## 10. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/mastra-scorer-evidence/README.md`
- `examples/mastra-scorer-evidence/requirements.txt` only if a tiny local
  generator truly needs it
- `examples/mastra-scorer-evidence/generate_synthetic_result.py` only if a
  small local generator is viable
- `examples/mastra-scorer-evidence/map_to_assay.py`
- `examples/mastra-scorer-evidence/fixtures/valid.mastra.json`
- `examples/mastra-scorer-evidence/fixtures/failure.mastra.json`
- `examples/mastra-scorer-evidence/fixtures/malformed.mastra.json`
- `examples/mastra-scorer-evidence/fixtures/valid.assay.ndjson`
- `examples/mastra-scorer-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional reference field
- v1 fixtures must not include trace trees, Studio metrics, or screenshots
- v1 fixtures must keep the export shape obviously scorer-result-first rather
  than observability-first

## 11. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

Mastra is promising because:

- scorer APIs are official
- datasets and experiments are now explicit first-class features
- CI execution is documented as part of the eval / scorer story

Even so, the first sample should still stay strict about setup cost.

### 11.1 Preferred path

Preferred:

- one tiny local scorer path
- one dataset-backed experiment only if it stays lightweight
- no cloud dependency
- no Studio dependency
- no observability plumbing heavy enough to dominate the sample

### 11.2 Hard fallback rule

If a real local generator would require:

- a large Mastra project bootstrap
- provider credentials
- brittle experiment orchestration
- heavy storage setup
- UI or observability setup that outweighs the sample itself

then the sample should fall back to a **docs-backed frozen scorer-result
artifact shape**.

That fallback is acceptable here because the goal is the smallest honest
external-consumer seam, not a full Mastra tutorial.

## 12. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 12.1 Valid

One strong scorer artifact with:

- bounded `dataset_version_ref`
- one bounded `item_ref`
- one bounded `score`
- `outcome=scored`

### 12.2 Failure

One weaker or failed scorer artifact with:

- the same bounded shape
- either a low score or one short `error_label`
- `outcome=failed`

This is **not** a platform failure artifact.

It is only a weaker or failed scorer-result artifact in the sample corpus.

### 12.3 Malformed

One malformed artifact that fails fast, for example:

- missing `score`
- missing `dataset_version_ref`
- unsupported `target_type`
- non-numeric `score` when the frozen sample shape requires numeric scoring

## 13. Outward strategy

Do not open an outward Mastra issue until the sample is on `main`.

After that:

- one small GitHub issue
- one link
- one boundary question
- no broad framework pitch
- no tracing pitch
- no dashboard pitch

Suggested outward question:

> If an external evidence consumer wants the smallest honest Mastra reliability
> surface, is a bounded scorer / experiment-item result artifact roughly the
> right place to start, or is there a thinner scorer result surface you would
> rather point them at?

## 14. Sequencing rule

This lane should still respect the current one-lane-at-a-time discipline.

Meaning:

1. let the freshest Browser Use, Visa TAP, and Langfuse lanes breathe unless a
   maintainer responds
2. formalize `P14` now
3. build the `P14` sample only if no hotter follow-up overrides it
4. open the Mastra issue only after the sample lands on `main`

## 15. Non-goals

This plan does not:

- define a Mastra trace export contract
- define a Studio metrics export contract
- define a dashboard export contract
- define legacy evals as the preferred first seam
- define Mastra scorer output as Assay truth
- define a platform synchronization story

## References

- [mastra-ai/mastra](https://github.com/mastra-ai/mastra)
- [Mastra Docs — Scorers / Evals overview](https://mastra.ai/docs/scorers/evals-old-api/overview)
- [Mastra Docs — Running in CI](https://mastra.ai/docs/evals/running-in-ci)
- [Mastra Blog — Introducing Scorers in Mastra](https://mastra.ai/blog/mastra-scorers)
- [Mastra Blog — Change, Run, and Compare with Experiments in Mastra Studio](https://mastra.ai/blog/mastra-experiments)
- [Mastra Blog — Announcing Datasets in Mastra Studio](https://mastra.ai/blog/announcing-datasets)
- [Mastra Blog — Announcing Metrics and Logs](https://mastra.ai/blog/announcing-studio-metrics)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
