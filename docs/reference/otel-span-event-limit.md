# OTel span-event limit: characterized behavior

This note characterizes how OTel-shaped evidence behaves around the span-event count limit, and
records the recommended exporter strategy. It is scoped narrowly.

**Claim:** Assay has characterized how its projected OTel-shaped evidence behaves around span-event
limits, and records the recommended exporter strategy.

**Non-claims:** Assay does not export OTLP live, is not fully OTel-compatible, does not preserve all
evidence through span-events, and this does not change `assay.otel_projection.v0` semantics.

## The load-bearing fact: the projection emits spans, not span-events

`assay project-otel` maps each observed tool and each policy decision to its own span (a tool span,
a guardrail span), with detail carried in span attributes. It never appends span-events to a span.
A `ProjectedSpan` has `name`, `kind`, and `attributes`, and no events array. So the OTel span-event
count limit does not apply to the projection as it stands today. High evidence volume becomes more
spans, not more events on one span. The regression guard for this invariant lives in
`crates/assay-core/tests/otel_projection.rs`
(`projection_maps_volume_to_spans_not_span_events`).

The limit matters for a future live OTLP exporter only if that exporter were to represent
high-volume correlated detail (many tool calls, many sub-steps) as span-events on a single span.
This note characterizes that path so the exporter design avoids the trap.

## Measured behavior

Measured with the Python `opentelemetry-sdk` 1.42.1. Full machine-readable result:
`crates/assay-core/tests/fixtures/otel_span_event_limit/result.v0.json`
(`schema: assay.experiment.otel_span_event_limit.v0`).

Default span-event limit (`OTEL_SPAN_EVENT_COUNT_LIMIT` / `SpanLimits.max_events` = 128):

| events added | events retained | events dropped | which were kept |
|--------------|-----------------|----------------|-----------------|
| 0            | 0               | 0              | n/a             |
| 1            | 1               | 0              | all             |
| 127          | 127             | 0              | all             |
| 128          | 128             | 0              | all             |
| 129          | 128             | 1              | the most recent 128 (oldest dropped) |
| 256          | 128             | 128            | the most recent 128 (oldest dropped) |
| 512          | 128             | 384            | the most recent 128 (oldest dropped) |

With `max_events = 512`, all counts up to 512 are retained with zero drops.

Two observations worth keeping:

1. Beyond the limit the SDK keeps the most recent N and drops the **oldest** events. For an audit
   trail that is the wrong end to lose: the earliest actions in a run are often the ones a reviewer
   most needs. Silent loss of the first events would make a long run look like it started later than
   it did.
2. This SDK does expose a `dropped_events` count on the in-memory span, so the loss is at least
   observable locally. Whether that count survives OTLP export and is surfaced by a given backend is
   a separate question this note does not test.

## Recommendation for a future live exporter

- Keep span-events for bounded timeline breadcrumbs only.
- Do not put high-volume evidence detail in span-events. The Assay source artifacts remain the
  source of truth; the projection should carry summaries, references, counts, and bounded samples,
  not an unbounded event stream.
- Continue mapping discrete evidence items (tools, decisions, effects) to their own spans and
  attributes, which is what the projection already does.
- For high-volume correlated detail, prefer log-based events with trace context, in line with the
  OTel direction of moving new event instrumentation toward logs while existing span-events stay
  compatible.
- If span-events are ever used, set and document the limit explicitly rather than relying on the
  default 128, and treat a non-zero `dropped_events` as an observation gap, never as clean.

## Non-claims

- This characterizes one SDK (Python `opentelemetry-sdk`), not all OTel SDKs; another SDK may keep
  the first N rather than the last N, or report drops differently.
- It does not prove backend-specific retention or drop reporting.
- It does not change `assay.otel_projection.v0` semantics.
- The projection emits spans, not span-events, so the limit does not apply to it today.

## Reproduction

No production dependency is added for this; the measurement runs in a throwaway virtualenv.

```bash
python3 -m venv .venv && . .venv/bin/activate
pip install opentelemetry-sdk
python3 characterize.py > result.v0.json
```

```python
# characterize.py
import json
from opentelemetry.sdk.trace import TracerProvider, SpanLimits

COUNTS = [0, 1, 127, 128, 129, 256, 512]
LIMITS = {"default": None, "configured_512": 512}

def measure(events_in, max_events):
    sl = SpanLimits() if max_events is None else SpanLimits(max_events=max_events)
    tracer = TracerProvider(span_limits=sl).get_tracer("char")
    span = tracer.start_span("s")
    for i in range(events_in):
        span.add_event(f"ev_{i}")
    span.end()
    names = [e.name for e in span.events]
    order = "n/a"
    if names:
        order = "first_n" if names[0] == "ev_0" else (
            "last_n" if names[-1] == f"ev_{events_in-1}" else "other")
    return {
        "events_in": events_in,
        "events_exported": len(names),
        "events_dropped": getattr(span, "dropped_events", None),
        "event_order_policy_observed": order,
        "first_kept": names[0] if names else None,
        "last_kept": names[-1] if names else None,
    }

default_max = SpanLimits().max_events
import opentelemetry.sdk.version as v
print(json.dumps({
    "schema": "assay.experiment.otel_span_event_limit.v0",
    "otel_sdk": {"language": "python", "package": "opentelemetry-sdk", "version": v.__version__},
    "default_max_events": default_max,
    "routes": {label: {"max_events": (default_max if lim is None else lim),
                       "cases": [measure(n, lim) for n in COUNTS]}
               for label, lim in LIMITS.items()},
    "non_claims": [
        "characterized behavior of one SDK (python opentelemetry-sdk), not all OTel SDKs",
        "does not prove backend-specific retention or drop reporting",
        "does not change assay.otel_projection.v0 semantics",
        "the assay projection emits spans, not span-events, so this limit does not apply to it today",
    ],
}, indent=2))
```
