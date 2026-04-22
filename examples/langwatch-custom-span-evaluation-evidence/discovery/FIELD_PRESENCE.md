# P25 LangWatch Custom Span Evaluation Discovery Notes

Date: 2026-04-22

Project used for live capture:

- local LangWatch docker stack on `http://127.0.0.1:5560`
- seeded project API key: `sk-lw-p25-local-test-key`

Capture path:

1. emit one custom evaluation through `langwatch.get_current_span().add_evaluation(...)`
2. fetch the first public surfaced trace representation via `langwatch.traces.get(trace_id)`
3. extract the child `evaluation` span from that surfaced trace response

The checked-in reduced sample is intentionally surfaced-derived, not emitted-
derived.

## Why the surfaced child evaluation span wins

The live discovery split is now clear:

- emitted input is the public SDK call shape we supplied
- surfaced trace response is the first public readback route that showed the
  evaluation again
- the useful bounded unit inside that surfaced trace response is the child
  `evaluation` span

That child `evaluation` span naturally carried:

- the evaluated span anchor as `parent_id`
- the evaluation payload inside `output.value`
- the surfaced time anchor inside `timestamps.finished_at`
- the trace reference as `trace_id`

So the reduced fixture is derived from that one surfaced child span rather than
from:

- emitted `add_evaluation(...)` input alone
- the wider trace envelope
- the top-level `evaluations` helper array

## Important live nuance

The top-level `evaluations` helper array in the trace response was not stable
enough for this first lane:

- for the `valid` capture it came back as `[]`
- for the `failure` capture it contained one helper object

By contrast, the child `evaluation` span was present and coherent in both live
captures.

That is why the reduced v1 lane is span-first and helper-array-agnostic.

## Field presence summary

| Field | Emitted input | Surfaced evaluation span | Top-level `evaluations` helper | Reduced v1 | Note |
| --- | --- | --- | --- | --- | --- |
| `name` | yes | yes | inconsistent | yes as `evaluation_name` | Direct evaluation identifier |
| `passed` | yes | yes in `output.value` | inconsistent | yes when present | First-class bounded result |
| `score` | yes | yes in `output.value` | inconsistent | yes when present | First-class bounded result |
| `label` | yes | yes in `output.value` | inconsistent | yes when present | First-class bounded result |
| `details` | valid: yes / failure: no | valid: yes / failure: no | inconsistent | optional | Kept only when present and bounded |
| `parent_id` | no | yes | no | yes as `entity_id_ref` | Natural evaluated-span anchor |
| `trace_id` | no | yes | trace-envelope only | optional as `trace_id_ref` | Natural reviewer aid on surfaced span |
| `span_id` | no | yes | no | no | Raw surfaced span id stays out of the reduced artifact |
| `timestamps.finished_at` | no | yes | helper timestamps inconsistent | yes as `timestamp` | Best surfaced time anchor in this sample |
| `output.type` | no | yes (`evaluation_result`) | n/a | no | Wrapper detail, not part of v1 |
| `output.value.status` | no | yes (`processed`) | inconsistent | no | Workflow/status semantics stay upstream |
| `params` | no | yes | no | no | Raw LangWatch instrumentation details are out of scope |
| `input` / `metrics` / `error` | no | yes (`null`) | helper-specific fields | no | Raw surfaced span scaffolding is out of scope |

## Observed vs reduced

Observed and kept:

- surfaced span parent id
- evaluation name
- any present `passed`
- any present `score`
- any present `label`
- any present bounded `details`
- surfaced `trace_id` as optional reviewer aid
- surfaced `timestamps.finished_at` as one reduced timestamp

Observed and dropped:

- raw surfaced `span_id`
- raw `type`
- raw `output` wrapper
- raw `status`
- raw `params`
- raw `input`
- raw `metrics`
- raw `error`
- wider trace envelope
- inconsistent top-level `evaluations` helper objects

## Packaging note

Running this capture against the public Python SDK on **2026-04-22** surfaced a
small packaging reality:

- `langwatch==0.22.0` was the current public SDK version at the checked-out
  upstream source
- the `add_evaluation(...)` path currently imports deeper evaluation and
  experiment modules at runtime
- in practice the probe environment also needed `pandas`, `tenacity`, and
  `tqdm`

Those extra dependencies are operational capture details only. They are not
part of the reduced evidence seam.
