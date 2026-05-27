# Runner vs OTel Overhead Findings Summary (2026-05)

> Last updated: 2026-05-27.

This is the citation-oriented summary of the Runner-vs-OTel overhead
follow-up. The full slice history, run table, and measurement details
remain in [`findings.md`](findings.md). Generated artifacts were
inspected as review evidence and are not committed as benchmark data.

## Scope

All claims below are scoped to the delegated
`linux-aarch64-6.8.0-117-generic` host class and the deterministic
agent workload used by this experiment. They are methodology and
measurement-boundary findings, not product rankings between
OpenTelemetry, OpenInference, or Assay-Runner.

The three arms are:

- **Arm A:** Runner archive capture only, without OTel trace export.
- **Arm B:** OTel-only trace export, without Runner archive capture.
- **Arm C:** dual capture: Runner archive capture plus OTel trace
  export.

Event-rate cell labels are defined by the overhead harness. In this
summary, `x1000` means 1000 kernel worker files, `s500` means 500
requested span events, `s1000` means 1000 requested span events, and
`corner-lite` combines 1000 kernel worker files, 1000 requested span
events, concurrency 8, and the large payload.

## Findings

### 1. Wall-clock does not decompose additively at this budget

The experiment does not publish an additive wall-clock split between
"Runner archive only" and "Runner archive plus OTel trace export." The
Arm A / Arm C median gap observed in separate phase-timing dispatches
did not reproduce under adjacent paired A/C diagnostics. At `n=20`, the
paired run produced noisy tails and close phase residual medians, so the
safe conclusion is that this wall-clock split is not stable enough to
publish as an additive decomposition on this runner.

This is a stopping rule, not a failure to measure anything: broad
single-arm reruns would mainly re-sample dispatch-window noise. Future
wall-clock work should target a narrower mechanism such as warm-up,
order effects, or a deeper child-runtime split.

### 2. RSS decomposes cleanly

Peak RSS is the stable decomposition result. At `n=5` RSS samples per
arm, Arm A runner-only and Arm C dual-capture had effectively identical
median RSS at this workload scale: `116,641,792 bytes` versus
`116,649,984 bytes`, a difference of `8,192 bytes` (`0.007%`). Arm B
OTel-only median RSS was
`108,953,600 bytes`.

The observed RSS increase from Arm B to Arm C is therefore attributable
to Runner capture rather than to adding OTel trace export around Runner
capture. This is the publishable memory-overhead finding from the arc.

### 3. Runner kernel capture stays healthy through x1000; OTel defaults cap spans at 128

The event-rate boundary sweep separated kernel-capture fidelity from
span-event retention. Runner kernel capture stayed healthy through
`x1000` kernel worker files and concurrency `16`: all measured samples
kept `ringbuf_drops=0`, `kernel_layer=complete`, and
`cgroup_correlation=clean`, with exact worker-file calibration in the
extracted archives.

The OTel span side hit a default SDK configuration boundary before any
throughput limit could be measured. With the checked-in workload's
default OpenTelemetry JS setup, widened span cells retained exactly
`128` events per span: `128/500` at `s500`, and `128/1000` at `s1000`
and `corner-lite`. This matches the OpenTelemetry
`SpanLimits.EventCountLimit` default of `128`. A local repro with
`OTEL_SPAN_EVENT_COUNT_LIMIT=1000` retained all requested events,
confirming this as a default-limit boundary rather than an Assay archive
or JSON writer loss.

Timing above 128 requested span events is not interpretable as
span-event throughput under default configuration, because the input
variable is clipped. Follow-up span-limit characterization is tracked
separately in [issue #1408](https://github.com/Rul1an/assay/issues/1408).

## What The Findings Mean Together

Assay and OTel are measuring different boundaries. Runner capture adds a
real and measurable memory cost, but it preserved kernel-capture health
through the widened event-rate cells. OTel trace export stayed small at
the baseline workload, but default span retention intentionally clips
large per-span event counts before a throughput boundary is reached.
Wall-clock does not support a neat additive story at this measurement
budget, and the experiment keeps that non-claim explicit.

The defensible position is therefore not "Runner is faster/slower than
OTel." It is: Runner capture provides fidelity-grounded out-of-band
evidence with an observable RSS cost; OTel default trace configuration
has a documented span-event retention boundary; and wall-clock deltas
for these arms require paired or narrower designs before they can be
interpreted.

## Reproduction Pointers

- Full evidence trail and run anchors:
  [`findings.md`](findings.md)
- Measurement plan and acceptance rules:
  [`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md)
- Harness, schemas, and local test command:
  [`README.md`](README.md)
- Schema reference:
  [`schemas-overview.md`](../../reference/runner/schemas-overview.md)
- Optional future OTel span-limit study:
  [issue #1408](https://github.com/Rul1an/assay/issues/1408)
