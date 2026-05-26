# Observability Join Contract v0

> **Status:** research/reference contract for the
> observability-layering line. This document defines how comparison rows
> may join traces, SDK events, policy events, measured-run archives, and
> external receipts. It is not a Runner archive artifact and not a
> product-facing API.

Join rows answer one narrow question:

```text
Which key connected two observability artifacts, and how much claim
strength may that join carry?
```

The contract prevents weak correlation keys from being treated as
semantic equality between layers.

## Schema String

```text
assay.observability.join_result.v0
```

Machine-readable schema:

[`schema/join-result-v0.schema.json`](schema/join-result-v0.schema.json)

## Join Hierarchy

| Rank | Key | Role | Grade |
|---:|---|---|---|
| 1 | `tool_call_id` | Primary join across trace, SDK, policy, and measured archive layers | `strong` when byte-equal and unique within the run. |
| 2 | `run_id` | Secondary run-level join across artifacts | `strong` for run pairing, not for per-tool semantics. |
| 3 | `session_id` | Contextual grouping only | `weak` unless combined with a stronger key. |
| 4 | `trace_span_id` | Trace-local propagation and diagnostic context | `weak` outside the trace artifact. |
| 5 | `timestamp_or_order` | Timestamp proximity or monotonic order fallback | `diagnostic` only. |

`trace_span_id` covers trace id, span id, and span-link based
correlation. These IDs are useful inside a trace, but they are not
automatically semantic equality across trace and measured-run artifacts.

## Join Grades

| Grade | Meaning |
|---|---|
| `strong` | The join can support a reviewable claim for the declared scope. |
| `weak` | The join can provide context, but not a strong claim by itself. |
| `diagnostic` | The join helps investigation only; it must not support a result claim. |
| `failed` | The artifacts could not be joined by the required key. |

## Scope

| Scope | Meaning |
|---|---|
| `tool_call` | The join identifies one tool-call interaction across layers. |
| `run` | The join identifies one run across artifacts, without per-tool equality. |
| `session` | The join groups related records but may contain many runs or tool calls. |
| `trace_local` | The join is meaningful only inside the trace. |
| `diagnostic` | The join is an investigative hint only. |

## Result Shape

```json
{
  "schema": "assay.observability.join_result.v0",
  "left_artifact_role": "otel_family_trace",
  "right_artifact_role": "measured_run_archive",
  "join_key": "tool_call_id",
  "join_value": "tc_runner_policy_001",
  "join_grade": "strong",
  "scope": "tool_call",
  "unique_within_scope": true,
  "fallback_used": false,
  "evidence_refs": [
    "trace.json",
    "layers/sdk.ndjson",
    "correlation-report.json"
  ],
  "notes": []
}
```

## Contract Principles

1. **Name the key used.** Every joined comparison row must say which
   key produced the join.
2. **Do not silently upgrade fallbacks.** Timestamp, order, and
   trace-local IDs remain diagnostic unless a stronger key also matches.
3. **Run joins are not tool joins.** A matching `run_id` pairs
   artifacts, but it does not prove that a trace tool span and measured
   kernel effect refer to the same tool call.
4. **Session joins are context.** A session can group records for
   review, but it is too broad for a strong per-call claim.
5. **Uniqueness is required for strong tool joins.** A `tool_call_id`
   that appears more than once inside the declared scope must not carry
   a strong join grade without an additional disambiguating key.
6. **Failed joins are evidence.** A missing or ambiguous join should be
   recorded as `failed` or `weak`, not dropped from the findings.

## Non-Claims

- This contract does not require OpenTelemetry or OpenInference to
  standardize `tool_call_id`.
- This contract does not claim trace/span IDs are weak inside a trace;
  it only limits their cross-layer meaning.
- This contract does not prove execution outcome.
- This contract does not prove policy correctness.
- This contract does not make archive content authoritative through the
  trace; digest binding must still be verified against the archive.
