# Runner Schema Overview

> **Status:** orientation index. This page does not define a new schema.
> It lists the current Runner-adjacent contracts, their stability scope,
> and whether they are archive evidence, projection output, or
> experiment-only measurement output.

## Namespace Rules

- `assay.runner.*` is for Runner archive, evidence, projection, or
  report contracts that may be consumed by Runner tooling.
- `assay.experiment.*` is for time-limited experiment evidence. These
  shapes may change between experiment slices and do not become stable
  Runner contracts unless a later reference page explicitly promotes
  them.
- Content-addressed evidence digests use `sha256:<hex>` style values.
  Git commit anchors use bare Git object IDs, full or abbreviated.

## Active Contracts

| Schema | Scope | Status | Sidecar |
|---|---|---|---|
| `assay.runner.archive_manifest.v0` | Runner archive manifest metadata | archive contract | described in [`artifacts-v0.md`](artifacts-v0.md) |
| `assay.runner.capability_surface.v0` | Capability surface snapshot inside archives | archive contract | described in [`artifacts-v0.md`](artifacts-v0.md) |
| `assay.runner.sdk_event.v0` | SDK-layer NDJSON events | archive contract | described in [`artifacts-v0.md`](artifacts-v0.md) |
| `assay.runner.kernel_event.v0` | Kernel-layer NDJSON events | archive contract | [`kernel-event-v0.schema.json`](schema/kernel-event-v0.schema.json) |
| `assay.runner.runtime_drift.v0.2` | Cross-runtime drift report emitted by the current comparator | projection/report contract | [`runtime-drift-v0.2.schema.json`](schema/runtime-drift-v0.2.schema.json) |
| `assay.runner.runtime_drift.v0` | Historical cross-runtime drift report shape | retained for older reports | [`runtime-drift-v0.schema.json`](schema/runtime-drift-v0.schema.json) |
| `assay.runner.path_projection.v0` | Additive path projection block embedded in runtime-drift reports | embedded projection vocabulary | described in [`runtime-drift-v0.md`](runtime-drift-v0.md) |
| `assay.runner.network_projection.v0` | Additive network projection block embedded in runtime-drift reports | embedded projection vocabulary | described in [`runtime-drift-v0.md`](runtime-drift-v0.md) |
| `assay.runner.runtime_noise_taxonomy.v0` | Shared vocabulary for runtime/provider/task/noise classes | vocabulary-only | described in [`runtime-drift-v0.md`](runtime-drift-v0.md) |
| `assay.runner.drift_report_provenance.v0` | Render and capture provenance embedded in runtime-drift reports | embedded report metadata | described in [`runtime-drift-v0.md`](runtime-drift-v0.md) |
| `assay.runner.cross_runtime_diff.v0.clean` | Earlier clean-output cross-runtime diff shape | reference schema | [`cross-runtime-diff-v0-clean.schema.json`](schema/cross-runtime-diff-v0-clean.schema.json) |

## Planned Experiment Schemas

| Schema | Scope | Status | Planned Sidecar |
|---|---|---|---|
| `assay.experiment.overhead_sample.v0` | One overhead measurement sample for runner-vs-OTel | plan-only | `docs/experiments/runner-vs-otel-overhead-2026-05/schema/overhead-sample-v0.schema.json` |
| `assay.experiment.overhead_summary.v0` | Aggregated overhead summary for runner-vs-OTel | plan-only | `docs/experiments/runner-vs-otel-overhead-2026-05/schema/overhead-summary-v0.schema.json` |

The planned overhead schemas remain experiment-scoped until the harness
exists, emits them, and sidecar tests validate them against synthetic
and live samples.

## Version Notes

`assay.runner.runtime_drift.v0.2` is a compatible tightening of the
runtime drift report shape, not a replacement for the raw archive
contracts. Consumers should parse dotted minor suffixes semver-like:
`v0.2` means major `0`, minor `2`. The current comparator emits v0.2
only; v0 is retained for historical reports already committed under
experiment runs.
