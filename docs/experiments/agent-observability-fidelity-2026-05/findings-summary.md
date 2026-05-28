# Agent Observability Fidelity Findings Summary (2026-05)

> Last updated: 2026-05-28.

This is the citation-oriented summary of the agent-observability
fidelity arc. The full slice history, scenario plans, harness details,
and delegated baseline record remain in
[`../agent-observability-fidelity-2026-05.md`](../agent-observability-fidelity-2026-05.md).
Generated synthetic outputs and delegated proof packs are review
artifacts; they are not promoted to product APIs by this summary.

## Scope

These findings are methodology and claim-boundary findings for Assay's
agent-observability layer. They cover experiment-scoped schemas,
synthetic harnesses, and one delegated positive-baseline smoke on the
existing `openai-agents-kernel-policy` Runner gate.

They do not rank Runner, OTel GenAI, OpenInference, or the OpenAI
Agents SDK as products. They also do not publish delegated semantic-gap
measurements: only the positive `matched_safe_read` join path has been
smoke-verified under real Runner capture.

The relevant layers are:

- **Trace layer:** reported agent/tool intent from OTel-style or
  OpenInference-style records.
- **Runner layer:** measured SDK, policy, cgroup, and kernel evidence in
  Runner proof packs or synthetic archive fixtures.
- **Join layer:** `assay.observability.join_result.v0` and
  `assay.observability.claim_class_cell.v0` rows that bound what a
  reviewer may claim from the trace/archive comparison.
- **Experiment carrier:** `assay.experiment.agent_observability_fidelity.*`
  artifacts that remain experiment-scoped unless a later promotion PR
  names a real consumer.

## Findings

### 1. Fidelity calibration is now a mechanical guardrail

The overhead arc's main warning was that an observability path can look
cheap when it silently stops retaining the requested signal. Slice 1
turns that warning into a reusable harness discipline: requested and
observed counts are represented as
`assay.experiment.agent_observability_fidelity.calibration.v0`, with
per-measurement `{target, observed, method, agreement}` entries and a
rollup `fidelity_verdict`.

The important distinction is `clipped` versus `drift`. `clipped` means a
known effective limit explains the loss, such as the OTel default
`SpanLimits.EventCountLimit=128`. `drift` means the observed count is
below target without a known clipping reason and needs investigation
before timing, throughput, or semantic absence claims are read from the
sample. That distinction moves calibration from reviewer memory into
schema-validated output.

### 2. Evidence packs carry claims without strengthening them

Slice 2 adds an experiment-scoped evidence-pack prototype with stable
filenames, a manifest, one-page summary, observation-health record,
optional trace, Runner archive/reference, and an explicit redaction
manifest. The pack records its own reproduction command and non-claims.

The key finding is not that evidence packs are now a product format.
They are not. The finding is that an Assay experiment can package a
bounded claim together with the artifacts needed to inspect it, while
stating that the carrier does not strengthen the underlying join,
health, calibration, or redaction evidence.

### 3. The semantic-gap matrix exercises bounded claim classes

Slices 3 and 4 define and implement six synthetic semantic-gap
scenarios: `matched_safe_read`, `path_rewrite`, `hidden_write`,
`retry_self_correction`, `runtime_side_effect`, and
`weak_join_fallback`. Together they exercise the important claim
classes: positive join, same-tool-call divergence, diagnostic fallback,
runtime-surface-only evidence, and inconclusive downgrade when health or
calibration is not clean.

The matrix is synthetic by design. Its value is that it makes the claim
rules executable: a strong `tool_call_id` join can support
`positive_join` or `semantic_gap`, while timestamp/order fallback stays
diagnostic. A measured effect mismatch is evidence of divergence between
reported intent and measured effect; it is not evidence of malicious
behavior, policy failure, or root cause by itself.

### 4. The interop matrix maps coverage, not product quality

Slices 5 and 6 add a synthetic interop matrix for OTel GenAI,
OpenInference, and Runner measured effects. The five starter cells emit
strict `interop_coverage_cell.v0` rows with source snapshots, mapping
basis, coverage status, claim strength, join-result references, and
claim-class references.

This makes absence first-class. A row with `coverage_status=absent` is
not a failed test when the vocabulary genuinely does not model the
behavior; it is a bounded coverage finding. Likewise, `partial` rows
document overlap without claiming semantic equivalence. The matrix is a
coverage and claim-strength map, not a translator and not a vendor
comparison.

### 5. The delegated positive baseline is smoke-verified

Slice 7 verifies the `matched_safe_read` positive baseline under real
Runner capture. GitHub Actions run
[`26571739019`](https://github.com/Rul1an/assay/actions/runs/26571739019)
passed the existing `openai-agents-kernel-policy` delegated gate,
uploaded proof pack `assay-runner-delegated-proof-pack-26571739019`,
and recorded clean Runner health: `kernel_layer=complete`,
`ringbuf_drops=0`, and `cgroup_correlation=clean`.

The delegated proof pack showed the expected
`tool_call_id=tc_runner_policy_001` path across SDK evidence, policy
evidence, workdir-bounded kernel read/open effects, a clean correlation
report, a strong join result, and a `positive_join` scenario verdict.
This verifies the positive join path before any delegated gap scenario
is published.

The delegated smoke also found and fixed an infrastructure bug:
systemd `.service` cgroups can be unsafe Assay session roots when their
cgroup type is threaded, so Runner now skips `.service` leaf units like
`.scope` units before creating session cgroups. That engineering
compliance proof is separate from the narrow Slice 7 research evidence.

## What The Findings Mean Together

Assay's useful whitespace is not "more overhead numbers." It is the
ability to say, for one agent run, what the trace reported, what the
system measured, which key joined those layers, whether the requested
signal was retained, what claim class is safe, and what portable
evidence a reviewer can inspect.

The defensible position is therefore not "Runner is better than OTel" or
"OpenInference is more complete than OTel GenAI." It is: Assay can
separate reported intent from measured effect, keep calibration and
health gates visible before claims are interpreted, and map external
trace vocabularies to bounded claim classes without pretending the
vocabularies or evidence boundaries are equivalent.

## Reproduction Pointers

- Full roadmap and slice history:
  [`../agent-observability-fidelity-2026-05.md`](../agent-observability-fidelity-2026-05.md)
- Semantic-gap scenario plan:
  [`semantic-gap-scenario-plan.md`](semantic-gap-scenario-plan.md)
- Synthetic semantic-gap harness:
  [`semantic_gap_harness.py`](semantic_gap_harness.py)
- Interop matrix plan:
  [`interop-matrix-plan.md`](interop-matrix-plan.md)
- Synthetic interop harness:
  [`interop_harness.py`](interop_harness.py)
- Delegated baseline smoke record:
  [`runs/slice7-delegated-baseline/summary.md`](runs/slice7-delegated-baseline/summary.md)
- Experiment schema governance:
  [`../../reference/experiments/namespace-governance.md`](../../reference/experiments/namespace-governance.md)
- Cross-namespace schema index:
  [`../../reference/runner/schemas-overview.md`](../../reference/runner/schemas-overview.md)
- Optional future OTel span-limit study:
  [issue #1408](https://github.com/Rul1an/assay/issues/1408)
