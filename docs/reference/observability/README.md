# Observability Reference Contracts

This directory contains small reference contracts used by Assay's
observability-layering experiments. They are review and research
contracts, not public product APIs unless a later ADR explicitly
promotes them.

The `assay.observability.*` namespace is for research/reference
vocabulary. It is not a Runner archive contract (`assay.runner.*`) and
not experiment-scoped measurement evidence (`assay.experiment.*`). See
[`../schemas-overview.md`](../schemas-overview.md) for cross-namespace
governance.

| Contract | Role |
|---|---|
| [`claim-classes-v0.md`](claim-classes-v0.md) | Vocabulary for saying what a trace, archive, or joined artifact can honestly claim. |
| [`join-contract-v0.md`](join-contract-v0.md) | Vocabulary for joining trace, SDK, policy, and measured-run evidence without silently upgrading weak keys. |

These contracts are intentionally separate from Runner archive schemas.
Runner artifacts remain the primary measured-run evidence. The
observability contracts describe comparison and interpretation output
above those artifacts.

The next planned experiment line for this layer is
[`agent-observability-fidelity-2026-05`](../../experiments/agent-observability-fidelity-2026-05.md).
It turns the completed overhead findings into a prioritized roadmap for
calibration guardrails, semantic-gap scenarios, portable evidence packs,
and OTel/OpenInference/Runner interoperability checks.

Experiment-scoped schema naming and promotion rules for that roadmap are
tracked in
[`../experiments/namespace-governance.md`](../experiments/namespace-governance.md).
Artifact-family status is tracked in
[`../artifact-families-inventory.md`](../artifact-families-inventory.md),
so proposed surfaces such as fidelity calibration, evidence packs, and
binding evidence do not get mistaken for canonical product artifacts.

The first experiment-scoped evidence-pack prototype is tracked in
[`../../experiments/agent-observability-fidelity-2026-05.md`](../../experiments/agent-observability-fidelity-2026-05.md).
It renders a portable manifest, one-page summary, observation-health
record, optional trace, Runner archive/reference, and explicit redaction
manifest without promoting that carrier to a product API.

The first semantic-gap scenario plan is tracked in
[`../../experiments/agent-observability-fidelity-2026-05/semantic-gap-scenario-plan.md`](../../experiments/agent-observability-fidelity-2026-05/semantic-gap-scenario-plan.md).
It predeclares how the join and claim-class contracts should be used
when trace-reported tool intent and measured system effects agree,
diverge, or only correlate weakly.

The first semantic-gap MVP harness is tracked in
[`../../experiments/agent-observability-fidelity-2026-05/semantic_gap_harness.py`](../../experiments/agent-observability-fidelity-2026-05/semantic_gap_harness.py).
It generates the full synthetic semantic-gap matrix and evidence packs
without promoting those outputs to delegated findings or product APIs.

The first interop matrix plan is tracked in
[`../../experiments/agent-observability-fidelity-2026-05/interop-matrix-plan.md`](../../experiments/agent-observability-fidelity-2026-05/interop-matrix-plan.md).
It predeclares how OTel GenAI, OpenInference, Runner measured effects,
and Assay claim vocabulary should be mapped as coverage and
claim-strength rows without ranking products or defining a runtime
translator.

The first synthetic interop harness is tracked in
[`../../experiments/agent-observability-fidelity-2026-05/interop_harness.py`](../../experiments/agent-observability-fidelity-2026-05/interop_harness.py).
It emits the five Slice 6 starter cells with `interop_coverage_cell.v0`
rows, join-result references, claim-class references, and source
snapshots without publishing delegated interop measurements.
