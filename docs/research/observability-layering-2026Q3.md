# Observability Layering Research Plan, 2026 Q3

> **Status:** planning artifact. This document defines the contracts,
> experimental arms, paper outline, and publication gates for a
> follow-up to the runner-vs-OTel work. It does not add capture code,
> publish benchmark numbers, change Runner archive semantics, or update
> product positioning.
>
> **Working title:** Measured-Run Archives Next to Live Traces: A
> Shape-by-Shape Comparison of Agent Observability
>
> **Preferred venue path:** blog-level rigor first, with optional arXiv
> promotion only if the data supports it.
>
> **Slice 1-2 status:** reference contracts for claim classes and joins
> are now drafted under [`docs/reference/observability/`](../reference/observability/).

## Research Question

What claims about an agent execution can be supported by:

- an OTel-family trace using OpenInference-style semantic conventions;
- an Assay-Runner measured-run archive;
- both artifacts joined by explicit keys; or
- neither artifact?

The experiment is about evidence boundaries, not product ranking. The
result should explain which layer carries which proof shape, where the
join is strong enough, and where a reviewer must not infer more than the
artifact can support.

## First Claim To Test

Traces explain the agent's reported control flow and context.
Measured-run archives bound observed system effects and measurement
health. They are complementary only when the join key is explicit and
the measurement boundary stays honest.

This is the claim the experiment must either support, narrow, or reject.
Do not update external positioning before the findings document exists.

## Non-Goals

- Not an "Assay vs OTel" comparison.
- Not a benchmark of OpenTelemetry, OpenInference, traceAI, or
  Assay-Runner as products.
- Not an AgentSight implementation or threat-detection claim.
- Not a model-quality, tool-quality, provider-latency, or runtime
  ranking benchmark.
- Not a claim that traces cannot carry content. Prompt, argument, and
  result capture are capture-policy choices.
- Not a claim that measured-run archives carry interaction content by
  default.
- Not a Trust Basis or Trust Card product surface.

AgentSight is relevant as motivation for the semantic gap between
high-level intent and low-level actions. This experiment targets
review evidence, not AgentSight's threat-detection problem.

## SOTA Anchors

The plan treats these as moving external anchors. Pin dates, versions,
or commit SHAs before publishing findings.

| Source | Used For | Publication Note |
|---|---|---|
| OpenTelemetry GenAI semantic conventions | OTel-family trace baseline and development-state context | Pin the docs date and, where possible, the `open-telemetry/semantic-conventions` commit. |
| OpenInference semantic conventions | v1 trace vocabulary above OpenTelemetry | Chosen for v1 because the capture target is independent of the outcome of OpenInference issue discussions. |
| traceAI | Optional v2 replication run | Do not mix into the v1 trace arm. |
| AgentSight | Motivation for high-level/low-level semantic gap and eBPF relevance | Cite only for the gap, not as an equivalent product goal. |
| Existing Assay Runner contracts | Measured-run archive, health, correlation, and capability-surface vocabulary | Link exact schema strings, [`../reference/runner/schemas-overview.md`](../reference/runner/schemas-overview.md), and Assay commit. |

## Claim Vocabulary v0

The claim classes table must use locked vocabulary rather than prose-only
qualifiers. Each claim cell has two axes:

```json
{
  "claim_strength": ["strong", "partial", "weak", "absent"],
  "claim_basis": ["reported", "measured", "derived", "inferred"]
}
```

### claim_strength

| Value | Meaning |
|---|---|
| `strong` | The artifact directly supports the claim inside its declared boundary. |
| `partial` | The artifact supports part of the claim, but another layer or assumption is needed. |
| `weak` | The artifact provides context or a hint, but not enough for a reviewable claim. |
| `absent` | The artifact does not support the claim. |

### claim_basis

| Value | Meaning |
|---|---|
| `reported` | The claim comes from an SDK, framework, trace, app hook, or another self-reported source. |
| `measured` | The claim comes from a measured runtime source such as cgroup-scoped kernel events or Runner observation health. |
| `derived` | The claim is computed from explicit source artifacts by a declared rule. |
| `inferred` | The claim depends on interpretation that is not directly carried by the source artifacts. |

`inferred` should be rare in findings. If a result needs `inferred`,
prefer writing it as a threat to validity rather than a main result.

## Claim Classes Table Skeleton

This table is the centerpiece. The findings document must fill it from
evidence, not from the plan.

| Claim type | OTel-family trace | Measured-run archive | Expected v1 interpretation |
|---|---|---|---|
| Reported control flow | `strong` / `reported` | `partial` / `reported` | Trace owns the readable control-flow story; archive may carry SDK side-band events but is not the trace. |
| Tool call intent and context | `strong` / `reported` when captured | `partial` / `reported` | Trace is better for intent and semantic context; archive should not become a prompt or payload store. |
| Tool call identity | `partial` or `strong` / `reported` | `strong` / `reported` when `tool_call_id` exists | Strong only when the same `tool_call_id` appears in both layers. |
| Policy decision evidence | `partial` / `reported` if instrumented | `strong` / `reported` when policy layer is present | A trace may show policy context; Runner binds policy events into the archive when present. |
| Measured filesystem effect | `absent` or `weak` / `reported` | `strong` / `measured` when health is clean | Kernel/capability-surface evidence carries this claim. |
| Measured network effect | `weak` / `reported` unless app spans include it | `strong` / `measured` when health is clean | Archive has the stronger bounded system-effect path. |
| Process execution effect | `absent` or `weak` / `reported` | `strong` / `measured` when health is clean | Trace normally does not carry process-exec truth. |
| Bounded negative claim | `weak` / `inferred` | `partial` or `strong` / `measured` | Only possible inside clean measurement boundaries; must cite health gates. |
| Measurement integrity | `absent` or `weak` / `reported` | `strong` / `measured` | Observation health, ring-buffer drops, and cgroup correlation are Runner-side claims. |
| Capability drift across runs | `partial` / `derived` | `strong` / `derived` | Archive diff is the intended review surface. |
| Privacy exposure of captured content | `partial` / `reported` | `strong` / `derived` for absence-by-design | Traces may carry content when configured; archives are bounded by design and do not carry interaction content by default. |

## Join Contract v0

The join hierarchy is part of the experiment contract.

| Rank | Key | Role | Claim Strength |
|---:|---|---|---|
| 1 | `tool_call_id` | Primary join across trace, SDK, policy, and measured archive layers | Strong when byte-equal and unique within the run. |
| 2 | `run_id` | Secondary run-level join across artifacts | Strong for run pairing, not for per-tool semantics. |
| 3 | `session_id` | Contextual grouping only | Weak unless combined with a stronger key. |
| 4 | trace id / span id | Trace-local propagation and diagnostic context | Not a cross-layer semantic join by itself. |
| 5 | timestamp proximity or tool order | Diagnostic fallback only | Must not support a strong claim. |

Any comparator output must name the join key it used. If it falls back
below `run_id`, the row must be marked weak or diagnostic.

## Run Modes

Use the same deterministic workload and host-class discipline across all
reported arms. `host_class` should use the same schema-safe format as
the overhead experiment (`^[A-Za-z0-9_.-]+$`) so cross-experiment
comparisons do not invent a second host fingerprint dialect.

| Arm | Capture | Purpose | Required For v1 |
|---|---|---|---|
| A0 | No capture | Absolute baseline for workload cost and perturbation | Yes |
| A1 | Runner archive only | Pure measured-run overhead and evidence shape | Yes |
| A2 | OpenInference trace only | Pure OTel-family trace overhead and evidence shape | Yes |
| A3 | Runner + OpenInference dual capture | Main shape comparison and join test | Yes |
| A4 | Runner + traceAI replication | Optional replication with a different OTel-native stack | No, v2 only |

A0 is required before any overhead claim. Without A0, the experiment can
say "dual capture differs from trace-only," but it cannot say what each
observation layer adds over the workload itself.

## Perturbation Metrics

Each reported arm should produce enough data to separate evidence shape
from capture overhead.

| Metric | Minimum | Applies To | Purpose |
|---|---:|---|---|
| End-to-end wall-clock runtime | n >= 20 per reported arm | A0-A3 | Absolute and relative perturbation. |
| Peak RSS | n >= 5 per reported arm | A0-A3 | Memory perturbation. |
| Trace span/event count | n >= 3 | A2, A3, A4 if used | Trace volume delta. |
| Archive event count and byte size | n >= 3 | A1, A3, A4 if used | Archive volume delta. |
| Capability-surface delta | n >= 3 | A1, A3, A4 if used | Whether capture mode changes observed effects. |
| Join success rate | every dual run | A3, A4 if used | Whether the evidence story can be joined without weak fallback. |
| Measurement health | every Runner run | A1, A3, A4 if used | Validity gate for measured claims. |

The wall-clock and RSS sample counts mirror the
[`runner-vs-otel-overhead-2026-05`](../experiments/runner-vs-otel-overhead-2026-05.md)
plan so the experiment lines use the same evidence bar.

A0-A3 with n >= 20 wall-clock samples means at least 80 captures before
shape samples and reruns. The workflow slice should budget for that
explicitly, with a timeout closer to 120 minutes than the default
30-minute experiment setting.

Do not report cross-host deltas as direct overhead. If A0/A1/A2/A3 do
not run on the same host class, publish host-class baselines instead.

## Privacy And Capture Policy

Traces optionally capture content: prompts, tool arguments, tool
results, retrieved documents, and model outputs. That can make traces
more useful for debugging and more exposed to content leakage.

Measured-run archives are bounded by design. They record system effects,
policy evidence, correlation, and measurement integrity. They do not
carry interaction content by default. If a future archive carries
payload-derived information, it should be an explicit projection,
digest, or redacted field with its own source and non-claims.

Findings must not say "trace has more, therefore trace is better" or
"archive lacks content, therefore archive is weaker." Capture policy is
part of the comparison.

## Workload Contract

For v1, reuse the existing deterministic workload contract from
[`cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md`](../experiments/cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md)
where possible. If the observability line needs a materially different
workload, freeze that as a v2 workload contract before capturing data:

- one agent invocation;
- one stable tool call with a stable `tool_call_id`;
- one policy decision when policy capture is enabled;
- one safe filesystem effect inside the workdir;
- no live network dependency unless the arm explicitly tests network
  effect capture;
- no prompt, tool argument, or result body capture by default;
- no streaming in v1 unless the workload contract is updated first.

If the existing workload does not support A0 cleanly, add A0 as a
minimal no-capture command path rather than emulating no-capture by
dropping artifacts after the fact.

## Evidence Layout

Proposed committed evidence path:

```text
docs/experiments/observability-layering-2026Q3/
  README.md
  findings.md
  schema/
    claim-class-cell-v0.schema.json
    join-result-v0.schema.json
    perturbation-sample-v0.schema.json
  runs/
    a0-baseline/
    a1-runner-only/
    a2-openinference-only/
    a3-dual/
    a4-traceai-replication/        # optional
  publication/
    blog-draft.md
    paper-outline.md
```

Step 0 creates this plan only. The schema sidecars are a follow-up
slice, but their enums are frozen here.

## Acceptance Gates

| Gate | Requirement |
|---|---|
| Contract freeze | Claim vocabulary and join contract are documented before capture code lands. |
| Same workload | A0-A3 run the same declared workload, with only capture mode changed. |
| Same host for deltas | Any reported delta comes from same-host or same-host-class arms. |
| Sample counts | n >= 20 wall-clock and n >= 5 RSS for overhead claims. |
| Health gates | Every Runner sample used for measured-effect claims has clean observation health. |
| Join disclosure | Every joined claim names the key used and marks weak fallbacks as weak. |
| Capture-policy disclosure | Findings say whether sensitive trace content capture was enabled. |
| Publication discipline | Positioning sweep waits until `findings.md` exists. If findings disprove or narrow the working claim, the positioning candidate must be revised, withdrawn, or explicitly limited in scope. |

## Suggested Slices

| Slice | Output | Gate |
|---|---|---|
| 0 | This research plan | Reviewed before code changes. |
| 1 | **Drafted:** `docs/reference/observability/claim-classes-v0.md` plus JSON schema sidecar | Enums match this plan; synthetic table validates. |
| 2 | **Drafted:** `docs/reference/observability/join-contract-v0.md` plus JSON schema sidecar | Strong/weak join grades validated on synthetic rows. |
| 3 | OpenInference capture infrastructure for existing workload | Trace-only A2 dry run emits stable trace shape without content by default. |
| 4 | Five run modes in workflow, A0-A3 required and A4 optional | Same workload and same host-class labels recorded; workflow timeout budget accounts for at least 80 required wall-clock captures. |
| 5 | Live captures with n >= 20 per required arm for overhead and n >= 3 for shape artifacts | Health gates pass for Runner arms. |
| 6 | `findings.md` with completed claim classes table | No direct deltas unless same-host-class rule is satisfied. |
| 7 | Positioning sweep informed by findings | README/scope/package descriptions cite claim classes rather than marketing assertions. |

## Paper Outline

### Abstract

One paragraph. State that traces and measured-run archives answer
different observability questions, and that the experiment compares
claim classes rather than products.

### Introduction

- Agent observability increasingly spans traces, semantic conventions,
  policy decisions, and runtime measurement.
- The hard question is not which artifact is better, but which claims
  each artifact can honestly support.
- Contributions: claim vocabulary, join contract, five-arm measurement
  design, and a measured shape comparison.

### Background

- OTel GenAI and OpenInference as reported control-flow/context layers.
- Assay-Runner measured-run archives as cgroup-scoped measured effects
  with observation health.
- AgentSight as evidence that high-level intent and low-level action can
  diverge, while noting this paper targets review evidence rather than
  threat detection.

### Method

- Workload contract.
- Run modes A0-A3, optional A4.
- Join contract.
- Capture-policy settings.
- Perturbation and evidence-shape metrics.

### Results

TODO after live capture:

- completed claim classes table;
- join success/failure rates;
- perturbation table;
- evidence volume table;
- examples where reported context and measured effects differ;
- negative claims supported only under clean measurement health.

### Threats To Validity

- Single workload in v1.
- OpenInference is one OTel-family choice, not all trace tooling.
- Capture-policy choices affect content visibility.
- Same-host discipline controls overhead claims.
- Runner/eBPF capture is Linux-specific.
- AgentSight comparison is motivational, not a reproduced threat model.

### Discussion

- What traces should keep owning.
- What measured-run archives should keep owning.
- Where explicit joins make the combined artifact stronger.
- Why archives should stay bounded by design and avoid becoming payload
  stores.

### Conclusion

Use only if supported by data:

```text
Traces explain reported control flow and context. Measured-run archives
bound observed system effects and measurement health. They are
complementary when joined by stable keys, and misleading when their
evidence boundaries are blurred.
```

## Positioning Rule

No README, package metadata, or external positioning update should claim
"Assay is the evidence compiler above traces" until the findings table
exists. After findings land, product copy may reference the result in
bounded language:

```text
Assay compiles measured runtime evidence and external trace or receipt
inputs into bounded review artifacts.
```

This sentence remains a positioning candidate, not a result of this
plan. It is conditional on the findings table supporting the
complementarity claim; if the findings narrow or reject that claim, this
sentence must be revised or withdrawn.
