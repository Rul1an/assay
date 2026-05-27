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
