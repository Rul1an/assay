# Delegated Semantic-Gap Expansion Plan

> **Status:** delegated-gap-expansion-plan-ready. This note predeclares
> the smallest post-closure delegated gap expansion after the successful
> `matched_safe_read` smoke. The plan itself did not dispatch a
> delegated run, add a new schema, promote experiment artifacts, or
> publish delegated semantic-gap findings.
>
> **Follow-up record:** implemented as one bounded sidecar in
> [`runs/delegated-hidden-write/summary.md`](runs/delegated-hidden-write/summary.md)
> and
> [`delegated-hidden-write-finding.md`](delegated-hidden-write-finding.md).
> The arc-level [`findings-summary.md`](findings-summary.md) remains
> closed.
>
> **Last updated:** 2026-05-29

## Goal

The delegated expansion asks one narrow question:

```text
After the positive matched_safe_read path is smoke-verified under real
Runner capture, can one same-tool-call gap scenario be repeated on the
delegated runner while preserving clean health, strong tool_call_id
joining, and bounded non-claims?
```

This is a technical publication gate for one future measured gap row,
not a new observability strategy and not a broad semantic-gap campaign.
The first delegated gap candidate is `hidden_write` because it keeps
the same single-call, strong-join shape as the positive baseline while
changing only the measured effect side.

This plan follows the post-closure follow-up rules in the
[`arc-lifecycle-guide`](../../reference/experiments/arc-lifecycle-guide.md#post-closure-follow-up-rules):
the arc-level findings summary stays closed, any successful delegated
gap result lands as a sidecar, and the follow-up remains bounded to one
predeclared technical gate.

## Prerequisites

| Prerequisite | Status | Why it matters |
|---|---|---|
| Positive delegated baseline | Done: [`runs/slice7-delegated-baseline/summary.md`](runs/slice7-delegated-baseline/summary.md) | Gap publication needs evidence that the non-gap join path works on the delegated host. |
| Full synthetic matrix | Done: [`semantic_gap_harness.py`](semantic_gap_harness.py) | The delegated scenario must map back to an already predeclared synthetic scenario id and verdict. |
| Existing delegated runner lane | Existing workflow: [`.github/workflows/runner-spike-delegated.yml`](../../../.github/workflows/runner-spike-delegated.yml) | The expansion should reuse the Linux/eBPF proof-pack mechanism instead of creating a parallel lane. |
| Join and claim-class references | Reference-ready | Delegated rows must reuse `assay.observability.join_result.v0` and `assay.observability.claim_class_cell.v0`. |
| Evidence-pack/proof-pack carrier | Prototype-ready | Reviewers need a stable directory or proof-pack reference, not loose logs. |

## First Candidate

`hidden_write` is the first delegated gap candidate.

| Property | Required value |
|---|---|
| Scenario id | `hidden_write` |
| Expected verdict | `semantic_gap` |
| Expected evidence-pack claim class | `semantic_gap` |
| Join key | `tool_call_id` |
| Join grade | `strong` |
| Reported layer | one `read_file` tool call with reported read intent |
| Measured layer | one workdir-bounded write/create effect in the same run window |
| Claim boundary | effect divergence at the same tool call; no maliciousness, policy-failure, or root-cause claim |

This is intentionally narrower than "run every gap scenario delegated."
It avoids the extra ambiguity in `path_rewrite` path projection, the
cardinality change in `retry_self_correction`, the run-scope
attribution problem in `runtime_side_effect`, and the intentionally weak
join in `weak_join_fallback`.

## Implementation Shape

The implementation PR should prefer a small fixture extension over a
new workflow surface. A suitable shape is:

```text
runner-fixtures/openai-agents/
  fixture-agent.js                    # existing deterministic fixture
scripts/ci/
  runner-spike-openai-agents-...sh    # existing delegated acceptance path
```

The future implementation may add a scenario selector or a narrow
acceptance wrapper, but it should keep these constraints:

- preserve the existing `matched_safe_read` delegated acceptance path;
- keep `hidden_write` opt-in so the existing baseline gate does not
  silently change behavior;
- keep the fixture deterministic across three delegated runs;
- emit the same proof-pack families already used by the baseline:
  selected JSON, SDK NDJSON, policy NDJSON, Runner archive tarballs,
  gate log, and proof-pack manifest;
- reuse the semantic-gap review rows rather than inventing a
  delegated-only verdict vocabulary.

If the fixture or acceptance script changes, the implementation PR must
rerun the positive baseline on the same head SHA. A delegated gap row is
not citeable if the positive baseline only passed on an older fixture
shape.

## Required Review Output

A future delegated `hidden_write` follow-up should produce one stable
review directory, for example:

```text
runs/delegated-hidden-write/
  proof-pack-reference.json
  join-result.json
  claim-class-cells.json
  scenario-verdict.json
  redaction-manifest.json
  summary.md
```

The directory may reference retained GitHub Actions proof-pack
artifacts rather than committing archive tarballs to git. If artifact
retention is time-limited, the summary must record enough run id,
artifact name, commit SHA, and digest metadata for re-dispatch
verification.

A successful delegated gap result should land as a sidecar finding, for
example `delegated-hidden-write-finding.md`, next to the review
directory. The existing arc-level
[`findings-summary.md`](findings-summary.md) remains closed and should
not be appended merely because one delegated gap row passes.

## Acceptance Rules

- Dispatch no delegated gap scenario until this plan, or a successor
  plan, is reviewed.
- Keep the first delegated expansion to `hidden_write` only.
- Run the positive `matched_safe_read` baseline on the same head SHA if
  fixture code, acceptance scripts, cgroup handling, SDK normalization,
  policy normalization, or kernel extraction changes.
- Require the delegated workflow to pass all deterministic runs for the
  selected gate or wrapper.
- Require clean Runner health: `kernel_layer=complete`,
  `ringbuf_drops=0`, `cgroup_correlation=clean`, and no correlation
  ambiguity that weakens the join.
- Require SDK evidence and policy evidence to name the same
  `tool_call_id` as the measured-effect binding.
- Require the joined row to use `join_key=tool_call_id`,
  `join_grade=strong`, `fallback_used=false`, and
  `unique_within_scope=true`.
- Require the measured write/create effect to stay inside the delegated
  fixture workdir. An effect outside the workdir fails the delegated gap
  gate rather than becoming a semantic-gap finding.
- Require `scenario-verdict.json` to use
  `scenario_id=hidden_write`, `verdict=semantic_gap`, and
  `evidence_pack_claim_class=semantic_gap`.
- If any required artifact is missing or health is not clean, classify
  the result as `inconclusive` with
  `evidence_pack_claim_class=diagnostic` and stop. Do not reinterpret
  the failure as a semantic gap.
- Do not use timestamp/order proximity to upgrade the claim if
  `tool_call_id` binding is absent or ambiguous.

## Later Candidates

Other synthetic gap scenarios remain in scope of the synthetic matrix.
Each delegated expansion beyond `hidden_write` requires its own
predeclared gate, review output, stop conditions, and non-claims before
dispatch.

## Stop Conditions

Stop the expansion without publishing a delegated gap finding if:

- the positive baseline cannot be re-established on the same head SHA
  after fixture or acceptance-script changes;
- the delegated host health is not clean;
- SDK, policy, and measured effects cannot be joined by a unique
  `tool_call_id`;
- the measured write effect cannot be bounded to the fixture workdir;
- the fixture requires broad workflow or lane changes that would make
  the proof about CI infrastructure rather than the semantic-gap claim.

In those cases, record an inconclusive technical outcome and keep the
synthetic matrix as the current semantic-gap evidence.

## Non-Claims

- This plan does not dispatch delegated gap scenarios.
- This plan does not publish a delegated `hidden_write` finding.
- This plan does not classify malicious behavior, root cause, policy
  failure, or tool poisoning.
- This plan does not rank Runner, OTel, OpenInference, the OpenAI
  Agents SDK, or any agent framework.
- This plan does not promote `semantic_gap_verdict.v0`,
  evidence-pack artifacts, join rows, or claim-class cells to product
  APIs.
- This plan does not require all synthetic semantic-gap scenarios to
  become delegated measurements.
