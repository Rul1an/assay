# Observability Fidelity Calibration

> **Status:** reference note. Last updated: 2026-05-28.
> This document does not open a new experiment arc, define a schema, or
> promote any `assay.experiment.*` artifact to a product API.

## Position

Observability fidelity calibration is a review discipline for deciding
whether an observation path retained enough of the requested signal for
downstream claims to be interpreted.

It is not a user-facing product SLO, SLA, uptime promise, support
promise, or vendor comparison. It answers a narrower question:

> Did the observation layer preserve the evidence it was asked to
> preserve, and if not, do we know why?

The closed Runner-vs-OTel overhead arc exposed the motivating failure
mode: an observation path can look cheap because a requested signal was
clipped before the measurement reached the layer being analyzed. The
agent-observability fidelity arc turned that warning into a mechanical
calibration discipline.

## Minimum Record Shape

A fidelity calibration record should be able to state, at minimum:

| Field | Role |
|---|---|
| `signal` | The requested observable, such as kernel worker files or span events. |
| `target` | The expected count, predicate, or retention target. |
| `observed` | The measured count or predicate result. |
| `method` | How the observed value was counted. |
| `agreement` | `match`, `clipped`, `drift`, `failed`, or `not_applicable`. |
| `effective_limit` | The known limit when clipping is explained. |
| `health` | Capture-layer health gates that affect interpretation. |
| `safe_claim` | The strongest claim the record supports. |

`effective_limit` is required when `agreement = clipped`. Without a
known effective limit, a lossy retained signal collapses to `drift`.

This is a reference shape, not a new schema. The experiment-scoped
implementation that proved the pattern is
`assay.experiment.agent_observability_fidelity.calibration.v0`.

For the separate Runner per-run measurement-health gate, see
[`../runner/fidelity-verdict-v0.md`](../runner/fidelity-verdict-v0.md).
That contract derives `clean`, `clipped`, `correlation_partial`,
`failed`, or `not_applicable` from one `observation_health.v0` record.
It is not this calibration `agreement` vocabulary: calibration compares
requested versus observed signals, while Runner fidelity gates what
claim types may be interpreted from a run's measurement health.

## Agreement Semantics

The central vocabulary is the agreement field:

- `match`: the requested signal is present and retained at the required
  level by the named method, and the recorded health gates do not
  contradict the retention claim.
- `clipped`: the signal existed or was requested, but the observation
  system truncated, dropped, sampled, or limited it for a known effective
  limit.
- `drift`: a usable artifact exists, but the count, shape, key, or
  category changed in a way that makes comparison unreliable and no
  known effective limit explains the change.
- `failed`: the capture path did not produce a usable artifact or
  measurement for the stated purpose.
- `not_applicable`: the signal is outside the measurement surface of the
  current layer, host, workload, or observation arm.

The important boundary is `clipped` versus `drift`. `clipped` is still a
lossy sample, but it is an explained lossy sample. `drift` is an
unexplained lossy sample and should block timing, throughput, and
absence claims until investigated.

The second important boundary is `drift` versus `failed`. `drift` leaves
a reviewable artifact, but makes comparison or interpretation unsafe for
the stated claim. `failed` means the measurement is not usable for that
purpose at all.

## Reading Rules

1. **Do not read speed from loss.** A sample that retained fewer events
   than requested may look faster because it did less observation work.
2. **Do not read absence from clipped capture.** If a trace path clipped
   at a known limit, missing later events do not prove absent behavior.
3. **Do not broaden absence claims.** Even `match` only authorizes
   negative claims inside the recorded capture layer, health state,
   configured limits, and measurement method.
4. **Do not upgrade drift into efficiency.** An unexplained count drop is
   a fidelity problem before it is a performance result.
5. **Do not hide health gates.** Ring-buffer drops, archive
   completeness, correlation health, and trace exporter health must stay
   visible because they can bound the safe claim.
6. **Attach the method.** A retained count is only reviewable when the
   record says how the count was produced.

## Worked Examples

| Scenario | Agreement | Why |
|---|---|---|
| Span events are capped by an exporter or SDK limit. | `clipped` | The signal was requested, but a known limit truncated the retained events. |
| A Runner archive reports `ringbuf_drops > 0` for the signal under review. | `failed` | Capture health contradicts a complete-retention claim for that purpose; absence or timing claims need a rerun or a narrower bound. |
| A tool span exists but the required cross-layer join key is missing. | `drift` | The trace artifact exists, but joined comparison has lost reliability. |
| Kernel-only capture is requested on an unsupported non-Linux host. | `not_applicable` | The signal is outside the current capture surface. |
| The expected worker-file count equals the observed archive count with clean health gates. | `match` | The requested signal was retained by the named method within the recorded health state. |

## Closed-Arc Examples

The overhead arc found that Runner kernel capture stayed healthy through
the widened event-rate cells, while the default OTel span-event path
retained exactly `128` events per span in cells that requested `500` or
`1000` events. The local repro with
`OTEL_SPAN_EVENT_COUNT_LIMIT=1000` retained all requested events, so the
safe classification was a known default-limit boundary rather than an
unknown throughput failure.

The fidelity arc converted that lesson into calibration rows with
per-measurement `{target, observed, method, agreement}` values and a
review-facing rollup. That made lossy capture visible before semantic
gap, interop, or timing claims were interpreted.

## What This Enables

Observability fidelity calibration lets a reviewer distinguish four
cases that otherwise collapse into one vague "we observed fewer events"
statement:

| Case | Safe interpretation |
|---|---|
| `target=1000`, `observed=1000`, `agreement=match` | Retention target was met. |
| `target=1000`, `observed=128`, `agreement=clipped` | Retention failed for a known limit. |
| `target=1000`, `observed=128`, `agreement=drift` | Retention failed for an unknown reason. |
| `target=1000`, `observed=None`, `agreement=failed` | Measurement failed; no retention claim is safe. |

The category is useful because the second and third rows require
different action. A clipped sample can support a configuration-boundary
finding. A drift sample requires investigation before it can support any
downstream measurement claim.

## Non-Claims

- This note does not define a product SLO, SLA, support promise, or
  availability surface.
- This note does not claim OTel, OpenInference, Runner, or Assay is
  better than another observation layer.
- This note does not publish new benchmark numbers.
- This note does not open the optional span-limit characterization issue
  as an active experiment.
- This note does not promote experiment-scoped calibration artifacts to
  product APIs.
- This note does not cover the separate evidence-pack discipline where a
  portable carrier must not strengthen the underlying claim.

## Source Anchors

- Runner-vs-OTel overhead findings:
  [`../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md`](../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md)
- Agent-observability fidelity findings:
  [`../../experiments/agent-observability-fidelity-2026-05/findings-summary.md`](../../experiments/agent-observability-fidelity-2026-05/findings-summary.md)
- Agent-observability fidelity roadmap and calibration fields:
  [`../../experiments/agent-observability-fidelity-2026-05.md`](../../experiments/agent-observability-fidelity-2026-05.md)
- Optional future span-limit characterization:
  [issue #1408](https://github.com/Rul1an/assay/issues/1408)
