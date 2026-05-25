# Cross-runtime drift — discussion comment draft

> **Status:** **not filed.** This is a comment draft that may be
> posted *as a reply* to
> [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162)
> **only if** OpenInference triage explicitly asks for a concrete
> example of how the four `agent.runtime_evidence.*` attributes
> behave across two different agent runtimes.
>
> If triage does not ask, this file stays on disk. The cross-runtime
> drift experiment is reachable through the blog draft and the
> repo evidence package; it does not need its own thread.
>
> **No new issue.** Do not file this as a separate question; that
> would violate the plan-doc's one-channel-at-a-time discipline.

## When to send this

The draft body below describes a *live* cross-runtime comparison.
The live Slice 3 baselines are now committed, so only one condition
remains before this comment goes out:

1. **A maintainer on #3162 asks** for a concrete example along
   the lines of "can you show what
   `agent.runtime_evidence.{health, boundary}` would carry
   across two different agent runtimes running the same task?"

Trigger to **not** post (regardless of baseline state):

- Triage routes us to OTel semconv — then mention this experiment
  in the routing comment as evidence, not as a new question.
- Triage says "vendor-specific, keep it private" — then nothing
  to add.
- Triage stays silent — keep waiting; the blog can publish without
  this comment.

## Comment body (proposed)

> Wanted to share a follow-up that might be useful triage context:
> we ran a sibling experiment that exercises the same four
> attributes across two different agent runtimes under the same
> Runner capture boundary (Linux/eBPF, cgroup v2).
>
> Same workload contract, two implementations:
> `@openai/agents@0.11.4` standard agent loop, and
> `@google/genai@2.6.0` with manual function-calling
> (`automaticFunctionCalling.disable = true`). Both produce a
> Runner archive with `observation_health.v0`,
> `capability_surface.v0`, and `layers/sdk.ndjson`.
> The committed baseline is n=3 per arm from
> <https://github.com/Rul1an/assay/actions/runs/26394765509>.
>
> Per-run, if an OTel/OpenInference trace were emitted alongside
> these archives, the four attributes from #3162 would carry the
> same shape in both arms:
>
> - `agent.runtime_evidence.digest` — `sha256:<manifest-bytes>`,
>   binds a trace to that arm's archive.
> - `agent.runtime_evidence.health` — `clean` (we gate every
>   iteration on `ringbuf_drops == 0`,
>   `kernel_layer == complete`, `cgroup_correlation == clean`
>   before upload).
> - `agent.runtime_evidence.boundary` — `linux_ebpf_cgroup_v2`,
>   identical in both arms by construction.
> - `agent.runtime_evidence.intent_effect_status` — runtime-agnostic;
>   the value depends on what the model did, not which SDK
>   dispatched the tool call.
>
> The interesting cross-runtime signal is one layer down: a
> per-dimension drift report computed from the two archives'
> capability_surfaces. Each row carries a downstream-comparator
> classification — task-induced, provider-induced,
> runtime-induced, inconclusive — that's intentionally distinct from
> `runtime_evidence.intent_effect_status` (which is a
> per-tool-call verdict). The four classification labels are
> downstream-comparator territory, not trace metadata, and we
> are explicitly NOT proposing to attach them to spans.
>
> In the live n=3 data, the stable labels are: filesystem and
> network rows `runtime-induced` under the v0 comparator, SDK tool
> surface and tool invocation order `task-induced`, process and MCP
> rows `inconclusive`. The network row is a useful limitation:
> the archive carries IP endpoints, not provider hostnames, so the
> comparator refuses to call that row provider-induced without a
> DNS/hostname binding.
>
> Full evidence + reproduction commands:
> <https://github.com/Rul1an/assay/tree/main/docs/experiments/cross-runtime-drift-2026-05>
>
> Sharing as datapoint, not as a new question — happy to leave
> the vocabulary question scoped to this issue.

## What this comment deliberately does NOT do

- Does not propose new trace attributes. The four labels are
  comparator-side, not trace-side.
- Does not ask for adoption or integration.
- Does not introduce a separate question.
- Does not @-mention any maintainer.
- Does not link a paper, a podcast, or a Twitter thread.

## What we deliberately do NOT do regardless of triage

- Do not file a new issue on `Arize-ai/openinference`.
- Do not file a new issue on `open-telemetry/semantic-conventions`
  unless OpenInference explicitly routes us there.
- Do not promote on Slack / Discord / X without a clear
  community signal that they want a broader audience.
