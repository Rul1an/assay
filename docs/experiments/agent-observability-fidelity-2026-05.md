# Agent Observability Fidelity Roadmap (2026-05)

> **Status:** plan-only follow-up after the completed
> Runner-vs-OTel overhead arc. This document ranks the next useful
> experiments for improving Assay's agent-observability surface. It does
> not dispatch new runs, does not commit measurement artifacts, and does
> not open the optional OTel span-limit study tracked in
> [issue #1408](https://github.com/Rul1an/assay/issues/1408).
>
> **Last updated:** 2026-05-27

## Executive Decision

The overhead arc is closed. The next valuable work is not another broad
wall-clock rerun. The useful whitespace is **fidelity-aware agent
observability**: making every trace/archive/receipt comparison say what
was requested, what was actually retained or measured, which layer
supports the claim, and where loss or semantic ambiguity begins.

Priority order:

0. **Experiment namespace governance** - pin naming, promotion, and
   cross-arc field rules before adding more observability artifacts.
1. **Fidelity calibration guardrails** - make requested-vs-observed
   counts first-class across Runner, OTel, and joined artifacts.
2. **Portable incident evidence packs** - turn one failing run into a
   bounded, reviewable evidence bundle.
3. **Semantic-gap experiments** - prove where reported trace intent and
   measured system effect diverge at the same tool call.
4. **Interop matrix** - compare OTel GenAI, OpenInference, and Runner
   evidence boundaries without pretending they measure the same thing.
5. **Optional OTel span-limit characterization** - only when an external
   consumer needs behavior above the default 128 span-event limit.

## Why This Direction

The latest overhead results produced three stable facts:

- Wall-clock decomposition between Runner capture and OTel trace export
  did not remain stable under paired A/C diagnostics.
- Peak RSS decomposed cleanly: Runner capture dominated the observed RSS
  increase, while OTel trace export added no measurable RSS at that
  scale.
- Runner kernel capture stayed healthy through 1000 worker files and
  concurrency 16, while default OpenTelemetry span retention clipped at
  `SpanLimits.EventCountLimit=128`.

The third result is the pivot. It shows that an observability system can
look efficient because it stopped retaining the requested signal. Assay
should therefore improve toward **calibrated fidelity** rather than raw
latency claims.

## SOTA Anchors

| Anchor | Relevance |
|---|---|
| [OpenTelemetry GenAI agent spans](https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/) | The GenAI agent conventions are still marked Development and include an opt-in path for latest experimental conventions. Assay should treat semantic versions and emitted convention families as measured configuration, not background context. |
| [OpenTelemetry Trace SDK Span Limits](https://opentelemetry.io/docs/specs/otel/trace/sdk/#span-limits) | `EventCountLimit` defaults to 128. This exactly matches the Slice 12 span-retention boundary and should be surfaced in samples before timing is interpreted. |
| [AgentSight: System-Level Observability for AI Agents Using eBPF](https://arxiv.org/abs/2508.02736) | Confirms the research direction for framework-agnostic, system-boundary observation of agents. Assay's differentiator is joining that boundary evidence to trace/receipt semantics and health gates. |
| [AgentTrace: A Structured Logging Framework for Agent System Observability](https://arxiv.org/abs/2602.10133) | Reinforces structured trace records as reliability and trust-calibration evidence, not just debugging logs. |
| [AgentSim: A Platform for Verifiable Agent-Trace Simulation](https://arxiv.org/abs/2604.26653) | Points toward verifiable, replayable trace corpora. Assay should make failing/interesting runs portable and inspectable. |
| [Beyond Black-Box Benchmarking](https://arxiv.org/abs/2503.06745) | Supports moving from pass/fail or product benchmarks to runtime-log and observability-driven analysis of agentic systems. |
| [OpenInference semantic conventions](https://www.mintlify.com/Arize-ai/openinference/python/semantic-conventions) | Provides a richer OTel-compatible AI/ML semantic layer to compare against OTel GenAI and Runner measured effects. |

## Step 0 - Experiment Namespace Governance

**Goal:** keep the next artifacts from becoming another set of
experiment-local one-offs.

The governance decision lives in
[`../reference/experiments/namespace-governance.md`](../reference/experiments/namespace-governance.md).
The artifact-family inventory lives in
[`../reference/artifact-families-inventory.md`](../reference/artifact-families-inventory.md).
Together they pin four rules before the calibration/evidence-pack work
begins:

- new experiment schema strings should prefer
  `assay.experiment.<arc_slug>.<artifact_slug>.v<N>`;
- promotion from `assay.experiment.*` to `assay.runner.*`,
  `assay.observability.*`, or a receipt family requires a real consumer
  or repeated cross-arc use;
- cross-arc fields such as `host_class`, `workflow_run_url`,
  `tool_versions`, and `calibration_status` should be repeated locally
  until multiple arcs prove the same nested shape.
- proposed artifact families such as fidelity calibration, evidence
  packs, binding evidence, semantic-gap findings, and interop mappings
  must stay visibly proposed until a promotion PR names a consumer.

This is intentionally a small docs step. It is not a schema promotion
and does not rename historical overhead artifacts.

## Experiment 1 - Fidelity Calibration Guardrails

**Goal:** make every measurement artifact self-report whether the
declared signal reached the observed layer.

> **Status:** harness-ready in the Runner-vs-OTel overhead package. The
> overhead harness now embeds
> `assay.experiment.agent_observability_fidelity.calibration.v0` in
> non-baseline sweep samples and summaries. It does not promote the
> calibration shape to a product API.

This is the immediate next code slice because it turns the Slice 12
lesson into a general guardrail. The overhead harness already records
`span_event_limit_effective`, `span_event_limit_source`, and
`span_event_limit_warning`; the next slice should generalize this into
observed-count fields and summary-level calibration gates.

### Proposed fields

| Field | Meaning | Layer |
|---|---|---|
| `target_kernel_events` | Requested kernel worker-file pressure | workload config |
| `observed_kernel_worker_files` | Unique `event-rate-sweep/worker-*` paths observed in `layers/kernel.ndjson` | Runner archive |
| `target_span_events` | Requested OTel span events | workload config |
| `retained_span_events` | Span events retained in trace JSON | OTel trace |
| `dropped_span_events_estimate` | `target_span_events - retained_span_events` when both are known | derived diagnostic |
| `span_event_limit_effective` | Effective OTel span event limit | OTel SDK config |
| `trace_semconv_family` | OTel GenAI / OpenInference convention family emitted | trace config |
| `calibration_status` | `clean`, `lossy`, `inconclusive`, or `not_applicable` | joined summary |
| `fidelity_verdict` | Review-facing rollup across OTel and Runner capture | calibration summary |
| `calibration_method` | How the observed count was produced | calibration metadata |
| `calibration_agreement` | `match`, `clipped`, `drift`, `failed`, or `not_applicable` | calibration decision |

### Acceptance rules

- A cell may not support timing, throughput, or scaling claims until
  calibration is `clean` or the finding is explicitly about loss.
- Lossy cells are still useful evidence, but only for fidelity-boundary
  statements.
- `calibration_status=inconclusive` must be visible in `summary.md`,
  not buried in artifacts.
- Arm A remains asymmetric: OTel span fields are `not_applicable` rather
  than zero-throughput evidence.
- Every observed count must name its method. Example methods:
  `kernel_ndjson_path_match_count`,
  `archive_contents_worker_files_count`, `otel_trace_json_events_count`,
  and `fixture_side_log_count`.
- The first schema should expose per-layer agreement, not only one
  summary boolean. A mixed cell can be `match` for kernel events and
  `clipped` for span events.
- `fidelity_verdict` should be a compact object for renderer/evidence
  pack readers, backed by per-measurement `{target, observed, method,
  agreement}` entries for auditability.

### Output

- **Done:** new experiment-scoped calibration sidecar under the overhead
  package.
- **Done:** unit tests covering sample/summary schema validation, OTel
  span-event counting, Arm A not-applicable behavior, span-limit
  clipping, and kernel worker-file counting.
- **Not done:** promotion into `assay.observability.*`. That still
  requires a non-overhead consumer or a later evidence-pack renderer.

## Experiment 2 - Portable Incident Evidence Pack

**Goal:** turn one interesting or failing agent run into a compact,
portable, reviewable bundle.

> **Status:** prototype-ready in the agent-observability fidelity
> package. The repo now includes
> `docs/experiments/agent-observability-fidelity-2026-05/evidence_pack.py`
> and strict v0 schemas for the pack manifest and redaction manifest.
> The prototype is experiment-scoped and does not promote evidence packs
> to a product API.

This is the first tool-facing slice after calibration because every
later experiment should be able to hand reviewers a bounded evidence
pack instead of a pile of raw artifacts. The first prototype should
target one existing controlled scenario, not a broad production
incident.

### Minimum bundle

| Required | Artifact |
|---|---|
| Yes | One-page Markdown summary |
| Yes | Runner archive or verified archive reference |
| Yes | Trace JSON or trace reference when a trace layer exists |
| Yes | Observation health summary |
| Yes | Redaction manifest, even if no redaction was applied |
| Nice-to-have v1 | Expanded manifest/provenance table |
| Nice-to-have v1 | Derived measured-effects summary |

### Prototype layout

The v0 generator writes a directory with stable filenames:

```text
manifest.json
summary.md
redaction-manifest.json
artifacts/<runner archive filename>
artifacts/observation-health.json
artifacts/trace.json          # only when a trace layer exists
```

The manifest uses
`assay.experiment.agent_observability_fidelity.evidence_pack.v0`. The
redaction manifest uses
`assay.experiment.agent_observability_fidelity.redaction_manifest.v0`.
`pack_id` is a deterministic digest over the carried input artifacts and
redaction manifest; rendered summaries are listed as artifacts but do
not create a circular pack-id dependency.

### Acceptance rules

- **Done:** the pack never strengthens a claim beyond the underlying
  join and calibration grades; v0 emits that as an explicit non-claim.
- **Done:** redaction is explicit. Even no-redaction packs include
  `redaction-manifest.json`.
- **Done:** the pack is reproducible from input artifacts by command,
  not hand-curated.
- **Done:** the first prototype uses stable filenames so later
  semantic-gap scenarios can reuse the same carrier.
- **Not done:** promotion into a canonical Assay bundle or evidence
  receipt family. That still requires a consumer and a promotion PR.

### Tool improvement

This should become the bridge from research evidence to a practical
Assay feature: "give me the portable evidence for this agent run."

## Experiment 3 - Semantic Gap / Intent-vs-Effect Benchmark

**Goal:** prove exactly where trace-reported intent, SDK events, policy
events, and measured system effects diverge.

This is the most strategically valuable new experiment. It extends the
existing runner-vs-OTel shape comparison and cross-runtime drift work
from "can we join layers?" to "what can the joined layers honestly
claim when they disagree?"

This experiment should come after the first evidence-pack prototype.
The gap scenarios are the argument; the pack is how the argument becomes
reviewable.

### Baseline decision to make before dispatch

Every semantic-gap scenario needs a non-gap baseline. The recommended
baseline is one deterministic safe tool call that emits the same
`tool_call_id` into trace/SDK/archive layers and whose measured effect
matches the reported intent. Synthetic ground truth is acceptable for
unit tests, but at least one delegated sanity run should prove the same
join path under real Runner capture before gap findings are published.

### Scenarios

| Scenario | Reported trace intent | Measured effect | Expected claim |
|---|---|---|---|
| Matched safe read | tool call reports reading `safe.txt` | kernel observes read of `safe.txt` | strong positive join |
| Argument/path rewrite | tool call reports `safe.txt` | kernel observes normalized alternate path | semantic mismatch at same tool call |
| Hidden write | tool call reports read-only action | kernel observes create/write in workdir | L1 underreports side effect |
| Retry/self-correction | trace records final successful action | kernel/archive records failed prior attempts | trace summary loses temporal evidence |
| Runtime side effect | no tool-level trace event | archive records runtime loader/config/probe path | runtime-induced surface |
| Weak join fallback | missing `tool_call_id`, only order/timestamp | effects are plausible but not strongly joinable | diagnostic-only claim |

### Acceptance rules

- Every row must emit an `assay.observability.join_result.v0` entry or a
  newer successor.
- Strong findings require unique `tool_call_id` or an explicitly
  equivalent key.
- Timestamp/order joins remain diagnostic and may not support semantic
  equality claims.
- The output must classify each scenario by claim class: reported
  intent, measured effect, joined evidence, or inconclusive.
- A measured effect mismatch is evidence of divergence. It is not by
  itself evidence of malicious behavior, policy failure, or root-cause
  attribution.

### Tool improvement

This experiment should drive product work on input-binding receipts and
per-tool evidence rows. If the tool cannot clearly say "same tool call,
different effect," the observability story is not strong enough yet.

## Experiment 4 - OTel / OpenInference / Runner Interop Matrix

**Goal:** compare semantic coverage across OTel GenAI, OpenInference,
and Runner measured effects without treating them as interchangeable.

### Matrix axes

| Axis | Values |
|---|---|
| Trace vocabulary | OTel GenAI current default, OTel latest experimental opt-in, OpenInference |
| OpenInference span kinds | `AGENT`, `LLM`, `TOOL`, `RETRIEVER`, `GUARDRAIL`, and related emitted kinds |
| Agent shape | single tool call, retry, handoff, retrieval/tool mix |
| Join key | `tool_call_id`, run id only, order/timestamp fallback |
| Evidence layer | trace-only, archive-only, joined |

### Acceptance rules

- The matrix reports coverage and claim strength, not product ranking.
- OTel GenAI convention version or opt-in value must be recorded.
- OpenInference package/version must be recorded.
- Missing fields are findings, not test failures, when the vocabulary
  legitimately does not model the behavior.

### Tool improvement

This should produce a map from external semantic conventions to Assay's
internal claim vocabulary. It informs importers, receipt families, and
docs around what Assay can honestly consume.

## Experiment 5 - Optional OTel Span-Limit Characterization

**Goal:** characterize span-event throughput/fidelity only after raising
the OTel SDK limit above the requested target.

This remains optional. It should not be opened just because the default
overhead arc found the 128-event boundary.

### External triggers

- A paper section needs a datapoint above the default cap.
- A user asks how OTel behaves at high span-event rates.
- An Assay feature becomes sensitive to traces with hundreds or
  thousands of events per span.

### Acceptance rules

- Set `OTEL_SPAN_EVENT_COUNT_LIMIT` above the requested target before
  dispatch.
- Verify retained event counts before interpreting timing.
- Any sample with `span_event_limit_warning` is non-citable for
  throughput above the effective limit.
- Keep this as a separate arc from the default-config overhead findings.

## Required Product Development From The Latest Experiments

These are not optional research niceties; they are engineering debt made
visible by the overhead and shape-comparison arcs.

1. **Observed-count metadata.** Samples and summaries need observed
   counts for kernel files, retained span events, dropped span events,
   and effective limits.
2. **Calibration status.** Every summary should say whether the input
   variable was actually observed before timing or scaling is discussed.
3. **Join-result ergonomics.** `assay.observability.join_result.v0`
   should become easier to emit from experiment comparators.
4. **Binding evidence / join receipts.** Tool input, tool result, trace
   id, archive digest, and measured effect need a bounded working shape,
   but this must not be framed as a product line until a promotion PR
   names the consumer.
5. **Evidence-pack renderer.** The repo needs a reproducible way to turn
   artifacts into a portable incident summary.
6. **Semconv/version capture.** OTel GenAI and OpenInference convention
   family/version must be recorded as effective config.
7. **Runner-health operations.** Delegated experiments depend on
   `assay-bpf-runner`; offline/backlog detection and recovery should
   remain part of the runbook.

## Recommended Slice Order

| Slice | Status | Purpose | Exit gate |
|---:|---|---|---|
| 0 | Done in this plan | Namespace governance for experiment artifacts | Naming, promotion, cross-arc field, calibration-method, and evidence-pack minimum rules are documented. |
| 1 | Harness-ready | Fidelity calibration fields and summary rendering | One overhead-style fixture proves clean, lossy, and not-applicable calibration states. |
| 2 | Prototype-ready | Portable evidence-pack prototype | `evidence_pack.py` emits the minimum pack: manifest, summary, archive/ref, optional trace/ref, health, and redaction manifest. |
| 3 | Planned | Semantic-gap scenario plan | Baseline plus six predeclared scenarios, claim classes, and join requirements documented before dispatch. |
| 4 | Planned | Semantic-gap harness | Fake fixtures plus one delegated sanity run prove joined intent/effect rows and evidence-pack output. |
| 5 | Planned | Interop matrix plan | OTel/OpenInference/Runner coverage axes and non-claims pinned. |
| 6 | Optional | OTel span-limit study | Only after an external trigger; otherwise remains issue-only. |

## Experiment vs Feature Boundary

Not every follow-up needs full experiment-arc discipline:

- **Experiment-like:** fidelity calibration, semantic-gap scenarios, and
  interop matrix. These need predeclared inputs, acceptance criteria,
  and closure rules.
- **Feature-like:** evidence-pack rendering and join-result ergonomics.
  These should iterate faster, but still preserve non-claims and
  validation fixtures.

Use the heavier slice discipline when the result will be interpreted as
evidence. Use feature iteration when the task is improving how evidence
is carried or rendered.

## What Not To Do Yet

- Do not start with the Interop Matrix. Without working calibration and
  evidence-pack output, a matrix is mostly a promise rather than
  inspectable evidence.
- Do not turn the required product-development list into one epic. Each
  item belongs to a different dependency chain.
- Do not open a new paper arc yet. The argument is strong, but the
  portable evidence format should exist before publication work starts.
- Do not start #1408 unless an external trigger appears.

## Closure Criterion

This roadmap is successful when Assay can take one agent run and answer:

```text
What did the trace report?
What did the system actually do?
Which key joined those layers?
Was the requested signal fully retained?
What claim class is safe?
What portable evidence can a reviewer inspect?
```

If the tool can answer those questions without hand-inspecting raw
artifacts, the next frontier becomes policy/eval integration. Until
then, more raw overhead runs are lower value than fidelity and
joinability improvements.
