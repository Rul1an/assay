# OpenInference / OTel GenAI semconv — filed issue

> **Status:** filed on 2026-05-25 as
> [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162).
> Discussions were not enabled on the target repo, so we filed an
> Issue under the `semantic conventions` umbrella instead. Same
> narrow question, same evidence link, same discipline.
>
> **No cross-post** to
> [`open-telemetry/semantic-conventions`](https://github.com/open-telemetry/semantic-conventions/issues)
> until OpenInference triage says "this lives at OTel."
>
> This file is kept as the source-of-truth copy of what was filed
> so the on-disk evidence package matches the public ask.

## Subject line

> [semantic conventions] Vocabulary for linking traces to runtime-evidence artifacts

## Body (as filed)

## Question

Where should an OTel/OpenInference trace put attributes that link a span/run
to an out-of-band runtime-evidence artifact captured alongside the same agent run?

We ran a controlled experiment where the same agent execution produced:

- an OTel/OpenInference-style trace for reported control flow
- a content-addressed runtime-evidence archive from Linux/eBPF + cgroup capture

The trace and archive are joined by `tool_call_id`, and the trace carries a
digest of the archive manifest so consumers can verify that both artifacts
describe the same execution.

Evidence package:
https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05

## Candidate attributes

We currently use private `assay.*` names, but do not want to squat on a namespace.
The concepts are:

- `agent.runtime_evidence.digest`
- `agent.runtime_evidence.health`
- `agent.runtime_evidence.boundary`
- `agent.runtime_evidence.intent_effect_status`

The last one came from a controlled tampering scenario: the trace reports tool
argument X, while the measured runtime evidence shows effect Y at the same
`tool_call_id`.

## Narrow ask

Should concepts like these live in OpenInference, OTel GenAI semantic
conventions, a separate namespace, or is there already an existing attribute
pattern we should use?

## Non-goals

- not asking OpenInference to adopt or integrate with Assay-Runner
- not asking maintainers to validate our eBPF implementation
- not proposing that traces should contain the runtime artifact itself
- not claiming OTel is missing something it was supposed to capture

---

## What we deliberately do NOT ask

Per the plan doc's external-outreach discipline:

- We do not ask for OpenInference / OTel to adopt or integrate with
  Assay-Runner specifically.
- We do not ask anyone to validate the experiment design or the
  Linux/eBPF mechanism. That's our problem.
- We do not multi-question dump.
- We do not @-mention any individual maintainer.

## Per-community channel discipline

| Repo | When to file |
|---|---|
| `arize-ai/openinference` Issues | Primary. Filed 2026-05-25 as [#3162](https://github.com/Arize-ai/openinference/issues/3162) under the `semantic conventions` umbrella. Discussions are not enabled on this repo, so an Issue was the correct channel. |
| `open-telemetry/semantic-conventions` Issues | Only as a cross-post if OpenInference triage routes us there. Do not double-file without a routing signal. |
| `open-telemetry/community` issues | Only if both OpenInference and semconv say "wrong place". |
