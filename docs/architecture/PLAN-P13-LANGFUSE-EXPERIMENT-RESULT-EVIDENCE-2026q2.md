# PLAN — P13 Langfuse Experiment Result Evidence Interop (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Planning
- **Scope (this PR):** Define the next platform-adjacent upstream interop lane
  after Browser Use and Visa TAP. No sample implementation, no outward
  Discussion, and no contract freeze in this slice.

## 1. Why this plan exists

After the current framework, protocol, runtime-accounting, eval-report,
adjacent output/history, and TAP verification wave, the next lane should still
pass the same three tests:

1. the upstream project already exposes one bounded surface,
2. Assay can consume that surface without inheriting upstream platform
   semantics as truth,
3. the repo has a natural maintainer channel for one small sample-backed
   boundary question.

`langfuse/langfuse` now fits that pattern well enough to justify a formal plan:

- the repo is large, active, and visibly current
- GitHub Discussions are enabled and actively used
- the docs expose a clear evaluation stack through datasets, experiments, and
  scores
- the platform is explicitly API-first and already frames data exports as part
  of the product story

This lane is especially relevant at the bleeding edge in 2026 because Langfuse
is evolving in two directions at once:

- **observations-first observability**, which makes trace surfaces broader and
  more operationally central
- **experiments-first evaluation workflows**, which make offline, bounded eval
  result surfaces easier to isolate from live production tracing

That combination creates a clear planning rule:

- do **not** open Langfuse as another trace-export lane
- do **not** pitch Assay as a competing observability platform
- do start from the smallest experiment-result seam that Langfuse already
  documents

This is **not** a trace export plan.

This is **not** a dashboard or metrics export plan.

This is **not** a prompt-management plan.

This is a plan for a **bounded experiment-result surface derived from the
documented Langfuse evaluation path**.

## 2. Why Langfuse is now the next best candidate

As of 2026-04-08, the current active and just-opened lanes already cover:

- framework trace surfaces
- eval-report surfaces
- runtime-accounting surfaces
- adjacent output/history surfaces
- commerce / verification-first protocol surfaces

That changes what “best next lane” means.

The next lane should now:

- stay GitHub-native
- open a different seam from the work already live
- still have strong momentum and a real maintainer channel
- be current enough to reflect where the ecosystem is actually heading

Langfuse is the strongest fit for that next move because:

- its evaluation stack is now clearly first-class, not a side feature
- its docs and SDK references already expose experiments, datasets, and custom
  scores as explicit product surfaces
- its Discussions categories include a natural answerable `Support` lane
- it is strategically important without forcing us straight into a browser,
  payment, or telemetry-heavy surface

Why this is **not** Mastra first:

- Mastra remains a good candidate
- but the repo has no Discussions
- and its issue channel is more bug and triage heavy right now

Why this is **not** x402 first:

- x402 remains technically interesting
- but the repo currently has no Issues and no Discussions
- and the public channel shape is too weak for the small sample-backed
  question style that is working best for us

Why this is **not** OpenLIT first:

- OpenLIT remains a valid OTel-native special case
- but it is smaller, and the lane would be much more observability-colored
  than the current Langfuse opportunity

## 3. Hard positioning rule

This lane must not overclaim what the sample actually observes.

Normative framing:

> This sample targets the smallest honest Langfuse experiment-result surface,
> not a trace export, dashboard export, metrics export, or production
> observability truth surface.

That means:

- Langfuse is the upstream platform context, not the truth source
- experiment results are bounded eval artifacts, not production truth
- Assay stays an external evidence consumer, not a scorer, dashboard, or trace
  authority

## 4. Why not trace-first

Trace-first would be the wrong first wedge here.

Why:

- Langfuse is publicly and product-wise strongest as an observability platform
- trace-first would read immediately as platform-on-platform
- the v4 observations-first direction makes live trace surfaces broader, not
  smaller
- Langfuse also supports live evaluators on production traces, which would make
  it too easy to blur runtime monitoring semantics into Assay evidence truth

The safer first seam is one step away from production observability:

- dataset-backed
- experiment-backed
- offline or reviewable
- smaller than traces
- smaller than dashboards
- smaller than metrics rollups

This is the same best-practice move we used in earlier lanes:

- choose the smallest documented surface that already exists
- avoid the broadest operational surface just because it is prominent

## 5. Why experiment-result-first and not score-only-first

Langfuse also exposes **scores** as a first-class concept:

- numeric, boolean, and categorical values
- custom scores via SDK or API
- scores attached across broader observability and evaluation workflows

That is real, but score-only is still not the best v1 seam.

Why not score-only first:

- scores can belong to traces, observations, or experiments
- score-only export loses the bounded experiment context that makes the seam
  reviewable
- score-only makes it easier to slide back into production-evaluation or
  observability semantics

The cleaner first wedge is:

- one artifact derived from `run_experiment()`
- one bounded `ExperimentItemResult`-style output
- one bounded list of `Evaluation`-style results
- optional `trace_ref` only if it is naturally present in the chosen sample
  shape

That still keeps Langfuse tied to scores and evaluation, but it does so inside
the smallest experiment-result frame instead of the broadest scoring frame.

## 6. Recommended v1 seam

Use **one frozen serialized artifact derived from the documented Langfuse
experiment result path** as the first external-consumer seam.

The intended upstream anchors are:

- `run_experiment()`
- `ExperimentResult`
- `ExperimentItemResult`
- bounded `Evaluation` results

The frozen sample shape should stay at the **item result** level, not the full
run export and not a whole trace tree.

The first artifact should therefore center on:

- one experiment name
- one dataset identity and one bounded dataset version reference
- one item reference
- one short output representation
- one bounded list of evaluations
- no aggregate layer unless it is naturally present and still small
- optional opaque run or trace references only if needed

Important framing rule:

> The sample uses a frozen serialized artifact derived from the documented
> Langfuse experiment-result surface, not a claim that Langfuse already
> guarantees one fixed wire-export contract for external evidence consumers.

## 7. v1 artifact contract

### 7.1 Required fields

The first sample should require:

- `schema`
- `platform`
- `surface`
- `experiment_name`
- `dataset_name`
- `dataset_version_ref`
- `item_ref`
- `timestamp`
- `output_ref`
- `evaluations`

These required fields belong to the frozen sample artifact shape.

They must be described as:

- sample-level reductions derived from documented Langfuse experiment results
- not an upstream guarantee that Langfuse already ships one canonical
  serialized export object with these exact field names

`dataset_version_ref` remains required in this v1 plan because it keeps the
artifact reviewable in dataset/experiment context, but that is still a frozen
sample-shape choice, not a claim that every smallest honest Langfuse export
must carry that exact field in that exact form.

### 7.2 Optional fields

The first sample may include:

- `run_ref`
- `trace_ref`
- `experiment_description_ref`
- `metadata_ref`
- `aggregate_scores`

### 7.3 Important field boundaries

#### `dataset_version_ref`

This field is required because dataset versioning is part of the documented
Langfuse experiments story.

In v1, it must stay a bounded version pointer only:

- timestamp
- short version label
- short normalized version reference

Not allowed in v1:

- full dataset export
- full dataset item payload replay
- dataset schema dump

This field belongs to the frozen sample shape, not to an upstream claim that
Langfuse guarantees one universal serialized dataset-version contract for
external consumers.

#### `item_ref`

This field is required and must stay an opaque dataset-item reference only.

Allowed:

- item id
- short item label
- short opaque reference

Not allowed in v1:

- full dataset input body as the primary seam
- full expected-output body as the primary seam
- large source-trace payloads

#### `output_ref`

This field is required in the frozen sample shape.

It should be framed as a **short frozen representation** derived from the
experiment item output, not necessarily the full upstream output body.

It should also be described explicitly as a **sample-level reduction of the
documented experiment item output surface**, not as an upstream-guaranteed
serialized export field.

It remains upstream output semantics only.

It must not become:

- model-quality truth
- prompt-quality truth
- business-outcome truth

The sample should prefer:

- short string output
- short bounded object
- short opaque output label

Not:

- full prompt transcript
- full chain-of-thought style payload
- large generated body

#### `evaluations`

This field is required in the frozen sample shape.

It should be framed as a bounded list derived from documented `Evaluation`
results, not as a claim that Langfuse already guarantees one universal
serialized eval-result wire contract.

Each evaluation entry should stay very small:

- `name`
- `value`
- optional short `data_type`
- optional short classifier or label only if the chosen sample shape naturally
  carries one

Not allowed in v1:

- evaluator prompts
- evaluator reasoning transcripts
- large free-text judge explanations
- raw dashboard rollups

This field is where Langfuse’s eval-result seam is real, but it must still
stay a bounded sample reduction rather than a platform export claim.

#### `trace_ref`

This field is optional in v1.

The sample should prefer omitting it unless it is naturally present in the
chosen experiment-result shape.

If present, it must remain:

- an opaque reference
- a short id
- a bounded link target

It must not turn the lane back into a trace-first plan.

#### `aggregate_scores`

This field is optional in v1.

The sample remains complete without any aggregate score field.

The v1 sample should prefer omitting it unless it is naturally present in the
chosen experiment-result shape and still stays very small.

If included, it must stay bounded:

- one short summary object
- one short scalar or small list

It must not become:

- dashboard export
- metrics export
- production health truth

#### Optional references

The optional reference fields must stay bounded:

- short label
- opaque id
- short normalized reference

Not allowed in v1:

- full trace trees
- prompt registry payloads
- full metadata blobs
- rich dashboard configuration

## 8. Assay-side meaning

The sample may only claim bounded experiment-result observation.

Assay must not treat as truth:

- production trace truth
- dashboard truth
- metrics truth
- prompt-quality truth
- model-quality truth beyond the observed eval artifact
- user-feedback truth beyond the observed score or label
- Langfuse platform semantics as a whole

Common anti-overclaim sentence:

> We are not asking Assay to inherit Langfuse trace semantics, dashboard
> semantics, metrics semantics, or evaluation semantics as truth.

## 9. Concrete repo deliverable

If this plan is accepted, the next implementation PR should add:

- `examples/langfuse-experiment-evidence/README.md`
- `examples/langfuse-experiment-evidence/requirements.txt` only if a tiny
  local generator truly needs it
- `examples/langfuse-experiment-evidence/generate_synthetic_result.py` only if
  a small local or self-hosted generator is viable
- `examples/langfuse-experiment-evidence/map_to_assay.py`
- `examples/langfuse-experiment-evidence/fixtures/valid.langfuse.json`
- `examples/langfuse-experiment-evidence/fixtures/failure.langfuse.json`
- `examples/langfuse-experiment-evidence/fixtures/malformed.langfuse.json`
- `examples/langfuse-experiment-evidence/fixtures/valid.assay.ndjson`
- `examples/langfuse-experiment-evidence/fixtures/failure.assay.ndjson`

Fixture boundary notes:

- v1 fixtures may omit every optional reference field
- v1 fixtures must keep `trace_ref` optional and preferably absent
- v1 fixtures must not include full trace trees or dashboard exports
- v1 fixtures should keep the export shape obviously experiment-result-first
  rather than trace-first

## 10. Generator policy

The implementation should prefer a real local generator **only if** it stays
small and deterministic.

Langfuse is more promising than some earlier lanes because:

- the platform is open source and self-hostable
- experiments can run against local or hosted datasets
- the experiment runner is now an official SDK concept

Even so, the first sample should still stay strict about setup cost.

Practical expectation for v1:

- start from a docs-backed frozen artifact shape by default
- only switch to a real local generator if the setup turns out to be
  surprisingly small and does not overshadow the seam

### 10.1 Preferred path

Preferred:

- a small SDK-driven experiment path
- one local or tiny self-hosted Langfuse setup only if it is truly lightweight
- one bounded evaluator
- no production-trace dependency
- no dashboard dependency
- no prompt-management dependency

### 10.2 Hard fallback rule

If a real local generator would require:

- a full Langfuse stack bootstrap heavy enough to dominate the seam
- cloud credentials
- project or environment setup that outweighs the sample itself
- more observability plumbing than experiment-result logic

then the sample should fall back to a **docs-backed frozen experiment-result
artifact shape**.

That fallback is acceptable here because the goal is the smallest honest
external-consumer seam, not a full Langfuse platform tutorial.

## 11. Valid, failure, malformed corpus

The first sample should follow the established corpus pattern.

### 11.1 Valid

One strong experiment item result with:

- short `output_ref`
- one or more bounded evaluations
- a bounded dataset version reference

### 11.2 Failure

One lower-scoring experiment item result with:

- short `output_ref`
- one or more bounded evaluations showing a lower or negative result
- the same bounded experiment-result shape

This is **not** a platform failure or infrastructure failure artifact.

It is only a weaker experiment-result artifact represented in the repo corpus
under the established `failure` naming pattern.

### 11.3 Malformed

One malformed artifact that fails fast, for example:

- missing `evaluations`
- missing `dataset_version_ref`
- `evaluations` not a list
- unsupported evaluation entry shape

## 12. Outward strategy

Do not open an outward Langfuse Discussion until the sample is on `main`.

After that:

- one small GitHub Discussion
- category: `Support`
- one link
- one boundary question
- no broad platform pitch
- no trace pitch
- no dashboard pitch

Why `Support`:

- it is the answerable category
- the question is about the smallest honest seam
- it is the best pragmatic fit for one technical boundary question even if it
  is not a perfect semantic match for a planning-style interop post
- that reads more like a technical boundary question than `Share your Work`
  or `Ideas`

Suggested outward question:

> If an external evidence consumer wants the smallest honest Langfuse
> evaluation surface, is an artifact derived from `run_experiment()` /
> `ExperimentItemResult` and bounded `Evaluation` results roughly the right
> place to start, or is there a thinner experiment-result surface you would
> rather point them at?

## 13. Sequencing rule

The active Browser Use and Visa TAP lanes are now live.

Meaning:

1. formalize `P13` now
2. let the fresh Browser Use and TAP outward threads breathe unless a
   maintainer responds
3. if no hot follow-up needs attention, implement the `P13` sample next
4. open the Langfuse `Support` Discussion only after the sample lands on
   `main`

## 14. Non-goals

This plan does not:

- define a Langfuse trace export contract
- define a Langfuse dashboard export contract
- define a metrics export contract
- define a prompt-management export contract
- define Langfuse evals or scores as Assay truth
- define a platform-to-platform synchronization story

## References

- [TODO — Next Upstream Interop Lanes (2026 Q2)](./TODO-NEXT-UPSTREAM-INTEROP-LANES-2026q2.md)
- [PLAN — P11A Visa TAP Intent Verification Evidence Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md)
- [PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md)
- [ADR-033: OTel Trust Compiler Positioning](./ADR-033-OTel-Trust-Compiler-Positioning.md)
- [Langfuse Docs — Overview](https://langfuse.com/docs)
- [Langfuse Docs — Evaluation Overview](https://langfuse.com/docs/evaluation/overview)
- [Langfuse Docs — Datasets / Experiments Overview](https://langfuse.com/docs/evaluation/experiments/overview)
- [Langfuse Docs — Support](https://langfuse.com/support)
- [Langfuse Docs — Roadmap](https://langfuse.com/docs/roadmap)
- [Langfuse Changelog — Experiment Runner SDK](https://langfuse.com/changelog/2025-09-17-experiment-runner-sdk)
- [Langfuse Python Reference — Experiment Runner](https://python.reference.langfuse.com/langfuse/experiment)
- [langfuse/langfuse](https://github.com/langfuse/langfuse)
