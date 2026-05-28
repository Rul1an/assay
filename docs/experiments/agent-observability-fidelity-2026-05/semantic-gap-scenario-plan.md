# Semantic Gap Scenario Plan

> **Status:** plan-only Slice 3 for the
> agent-observability fidelity roadmap. This document predeclares the
> baseline, scenarios, join requirements, claim classes, and evidence
> pack expectations before any semantic-gap harness or delegated run is
> added.
>
> **Last updated:** 2026-05-28

## Goal

The semantic-gap experiment asks one narrow question:

```text
When a trace reports one tool-call intent and Runner measures a system
effect, what claim is safe if those layers agree, disagree, or can only
be joined weakly?
```

This is not an overhead benchmark. It is a fidelity and claim-boundary
experiment that uses the completed calibration guardrail and evidence
pack prototype as prerequisites.

## Prerequisites

| Prerequisite | Status | Why it matters |
|---|---|---|
| Fidelity calibration | Done for the overhead harness | A trace/archive comparison cannot interpret missing retained signal as efficient or safe behavior. |
| Evidence pack carrier | Prototype-ready | Every scenario should be reviewable as a small pack rather than a loose artifact pile. |
| Join contract | Reference-ready | Strong findings require an explicit join key and grade, not timestamp proximity. |
| Claim classes | Reference-ready | Reported intent, measured effects, derived joins, and inferred diagnostics must stay separate. |

The first harness PR should reuse
`assay.observability.join_result.v0`,
`assay.observability.claim_class_cell.v0`, and
`assay.experiment.agent_observability_fidelity.evidence_pack.v0` unless
the implementation proves a version bump is required.

## Baseline

The baseline is a deterministic safe tool call:

| Field | Value |
|---|---|
| Scenario id | `matched_safe_read` |
| Tool call id | stable unique id, for example `tc_semantic_gap_001` |
| Reported intent | read `safe.txt` |
| Measured effect | kernel/archive observes read/open of `safe.txt` inside the workdir |
| Expected join | `tool_call_id`, `strong`, `tool_call`, `unique_within_scope=true` |
| Expected claim | positive joined evidence: reported intent and measured effect agree inside the measurement boundary |

This baseline is not optional. Every gap scenario is interpreted against
the same fixture contract and the same join path. Synthetic fixtures are
acceptable for unit tests, but at least one delegated sanity run must
prove this baseline under real Runner capture before any gap finding is
published.

## Scenario Matrix

| ID | Role | Reported trace intent | Measured system effect | Join requirement | Expected safe claim |
|---|---|---|---|---|---|
| `matched_safe_read` | baseline | tool reports reading `safe.txt` | archive observes read/open of `safe.txt` | unique `tool_call_id` | strong positive join |
| `path_rewrite` | gap | tool reports `safe.txt` | archive observes normalized alternate path that resolves inside the same fixture boundary | same unique `tool_call_id` | semantic mismatch or projection ambiguity, not unsafe behavior |
| `hidden_write` | gap | tool reports read-only action | archive observes create/write of `side-effect.txt` in workdir | same unique `tool_call_id` | reported intent under-describes measured side effect |
| `retry_self_correction` | gap | trace summary records final successful read | archive records prior failed attempts before the final read | same unique `tool_call_id` plus ordered attempt index if available | trace summary loses temporal evidence |
| `runtime_side_effect` | gap | no tool-level event reports the runtime/config/probe path | archive observes runtime loader/config/probe path inside capture boundary | run-level join only unless a tool id exists | runtime-induced measured surface; diagnostic unless scoped to runtime setup |
| `weak_join_fallback` | fallback | tool event is missing `tool_call_id` | archive observes plausible matching effect near the same order/timestamp | timestamp/order only | diagnostic-only correlation, not semantic equality |

### Scenario Notes

- `path_rewrite` should distinguish benign projection mismatch from
  dangerous target drift. A normalized path inside the fixture boundary
  is still a gap between reported and measured representation, not
  automatically a policy failure.
- `hidden_write` is the sharpest same-tool-call divergence. It needs a
  clean Runner health gate and a unique tool-call join before it can
  support a strong joined-evidence claim.
- `retry_self_correction` should keep prior failed attempts visible even
  when the final trace span reports success. The point is temporal
  loss, not whether retry behavior is good or bad.
- `runtime_side_effect` is intentionally not framed as agent intent. It
  tests whether Assay can separate tool effects from runtime/framework
  effects.
- `weak_join_fallback` exists to prove the negative case: plausible
  timing is useful for investigation but must not become a strong claim.

## Required Outputs

The harness slice should produce one output directory per scenario with
stable names:

```text
semantic-gap-runs/<scenario-id>/
  join-result.json
  claim-class-cells.json
  evidence-pack/
    manifest.json
    summary.md
    redaction-manifest.json
    artifacts/...
```

Minimum required rows per scenario:

| Row | Requirement |
|---|---|
| Join result | One `assay.observability.join_result.v0` row naming the key, grade, scope, uniqueness, fallback usage, and evidence refs. |
| Claim cells | At least one trace/reported cell, one archive/measured cell, and one joined-artifacts cell. |
| Evidence pack | One experiment-scoped evidence pack carrying the trace/archive or references, observation health, redaction manifest, and one-page summary. |
| Scenario verdict | A bounded verdict: `positive_join`, `semantic_gap`, `diagnostic_only`, or `inconclusive`. |

Evidence-pack `claim_class` should map verdicts conservatively:

| Scenario verdict | Evidence-pack `claim_class` |
|---|---|
| `positive_join` | `positive_join` |
| `semantic_gap` | `semantic_gap` |
| `diagnostic_only` | `diagnostic` |
| `inconclusive` | `diagnostic` |

## Claim Rules

| Condition | Maximum safe claim |
|---|---|
| Unique `tool_call_id`, clean Runner health, and matching reported/measured target | `positive_join` |
| Unique `tool_call_id`, clean Runner health, and measured effect differs from reported intent | `semantic_gap` |
| Clean Runner health but only run-level join | measured effect exists in the run; no per-tool semantic equality |
| Timestamp/order fallback only | diagnostic-only |
| Runner health not clean | inconclusive for measured-effect claims |
| Trace calibration lossy or inconclusive | no claim that absent trace fields prove absence of intent |

The first findings document should report claim strength and basis using
`assay.observability.claim_class_cell.v0` vocabulary:

| Layer | Typical basis | Typical strength |
|---|---|---|
| Trace intent | `reported` | strong inside trace boundary, absent for unreported effects |
| Runner archive effect | `measured` | strong only when health is clean |
| Joined comparison | `derived` | bounded by join grade and the weaker source layer |
| Fallback/order correlation | `inferred` | weak or diagnostic only |

## Acceptance Rules

- Do not dispatch or publish measurements from this plan-only slice.
- Every scenario must have a role: `baseline`, `gap`, or `fallback`.
- Strong semantic-gap findings require a unique same-scenario
  `tool_call_id`; timestamp/order fallback remains diagnostic.
- Every measured-effect claim must state Runner health and evidence refs.
- Every trace absence claim must state trace retention/calibration
  status. Missing trace fields do not prove missing behavior.
- Each scenario evidence pack must preserve the non-claim that it does
  not strengthen underlying join/calibration grades.
- Redaction must remain explicit even for synthetic fixtures.
- Mismatches are divergence evidence, not proof of malicious behavior,
  policy failure, or root cause.

## Non-Claims

- This plan does not add a harness or dispatch delegated runs.
- This plan does not rank OTel, OpenInference, Runner, or Assay.
- This plan does not claim semantic gaps are malicious.
- This plan does not promote evidence packs, join results, or claim
  cells to product APIs.
- This plan does not replace Runner archive integrity or health gates.

## Exit Gate For Slice 4

Slice 4 may start when this document is merged and the harness PR can
show, using synthetic fixtures first, that:

1. `matched_safe_read` emits a strong `tool_call_id` join and a
   `positive_join` evidence pack.
2. `hidden_write` emits a strong join but a `semantic_gap` verdict.
3. `weak_join_fallback` emits only a diagnostic join and cannot be
   rendered as semantic equality.

Those three cases are the minimum useful harness. The remaining
scenarios can be added after the shape is proven, but the harness should
not publish delegated findings until all predeclared scenarios have
either run or been explicitly scoped out.
