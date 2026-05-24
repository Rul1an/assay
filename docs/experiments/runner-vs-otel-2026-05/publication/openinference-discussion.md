# OpenInference / OTel GenAI semconv — discussion draft

> **Status:** draft, ready to file as a Discussion on
> [`arize-ai/openinference`](https://github.com/Arize-ai/openinference/discussions)
> (and optionally cross-linked to
> [`open-telemetry/semantic-conventions`](https://github.com/open-telemetry/semantic-conventions/discussions)).
>
> **Intent:** narrow vocabulary question, *one* discussion, *one*
> attached evidence link, *no* product pitch. Per the plan doc's
> "External question packet" discipline.
>
> **Not yet filed.** Maintainer is reviewing before posting; revisions
> live here so the version sent matches what we'll defend.

## Subject line

> Vocabulary question: how should an OTel/OpenInference trace refer
> to a content-addressed runtime-evidence artifact captured alongside
> the same agent run?

## Body

Hey folks,

Was reading through how OpenInference handles agent tool spans
(the `tool.call.id` propagation is exactly the join key I needed)
and wanted to ask one vocabulary question before I default to a
private namespace.

Sibling project does a Linux/eBPF measured-run capture for agent
runs. Three slices of evidence committed in the repo: per-run
`sha256` binding from an OTel span event to an out-of-band
runtime-evidence archive, tool-level join between
`gen_ai.tool.call.id` and the archive's SDK layer, and a
controlled tampering scenario where the trace reports reading
file X and the kernel observes a read of file Y at the same
`tool_call_id`. All on real eBPF data with n=3 per arm.

Need four attributes on the trace side so a consumer can verify
the binding and read the intent-vs-effect verdict without owning
the archive content. They sit in a private `assay.*` namespace
today because squatting on unspecified OTel/OpenInference names
felt wrong.

Candidates:

- `agent.runtime_evidence.digest`: content-addressed pointer,
  `sha256:<bytes>` over the archive's manifest
- `agent.runtime_evidence.health`: measurement integrity
  (`clean`, `degraded`, `unknown`)
- `agent.runtime_evidence.boundary`: declared measurement
  boundary, e.g. `linux_ebpf_cgroup_v2`
- `agent.runtime_evidence.intent_effect_status`:
  reported-vs-measured verdict per tool call

Narrow question:

> Where should attributes like these live? OpenInference spec,
> an OTel GenAI extension, a separate namespace, or is there
> something existing I should use instead?

Three answers are all useful signal: "fits under OpenInference,
here's the section", "belongs in OTel GenAI semconv, file it
there", or "this is a different abstraction, keep it private and
link from your namespace". Any of those settles it.

Full evidence and reproduction commands:

- Experiment package: <https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05>
- Plan and claim taxonomy: [`runner-vs-otel-shape-comparison-2026-05.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-shape-comparison-2026-05.md)
- v1 findings with the four slice resolutions: [`v1-findings.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/v1-findings.md)
- Comparator (stdlib Python, 17 unit tests): [`compare/compare.py`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/compare/compare.py)
- Per-run baselines under `runs/{v1-arm-c, slice2-arm-c, slice3-arm-c}/`

For transparency: Linux/eBPF only on the producer side, runtime
crates publish to crates.io with explicit internal/experimental
package descriptions, no third-party users, no integration ask.
Just want the names to land in the right place.

Nice spec by the way, the Python / TS / Java / C# breadth shows
real instrumentation work and came through clearly when I was
reading it.

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
| `arize-ai/openinference` Discussions | Primary. The four attributes naturally live in agent/tool observability space. |
| `open-telemetry/semantic-conventions` Discussions | Optional cross-link if the OpenInference maintainers route us there. Do not double-file without a routing signal. |
| `open-telemetry/community` issues | Only if OpenInference + semconv both say "wrong place". |
