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

Hi OpenInference / OTel GenAI WG,

We ran a controlled experiment (Linux/eBPF, n=3 per arm) that
captures an agent run twice in the same execution:

- **L1 — reported control flow**: OTel/OpenInference-style trace
  emitted in-process by the workload using
  `@opentelemetry/sdk-trace-base` and the OTel GenAI semconv
  attribute set (`gen_ai.provider.name`, `gen_ai.operation.name`,
  `gen_ai.tool.name`, `gen_ai.tool.call.id`, optionally
  `gen_ai.tool.call.arguments`).
- **L2 — measured runtime evidence**: a content-addressed Runner
  archive produced by an eBPF + cgroup-v2 capture that records the
  kernel-observed filesystem paths, network endpoints, process
  execs, plus measurement-health gates (`ringbuf_drops`,
  `cgroup_correlation`).

Both streams share the same `tool_call_id`, and a per-run
`sha256:<manifest-bytes>` digest of the archive's `manifest.json`
is attached to the trace's root span as an
`assay.archive.created` event — a tamper-evident binding in the
SLSA-provenance style. This lets us state per-run claims like
"this trace and this archive describe the same execution" without
the trace owning the archive's content.

We've also instrumented a controlled adversarial scenario where
the agent reports reading file X and the kernel observes a read
of file Y. With L1 and L2 joined on `tool_call_id`, the
comparator expresses the asymmetry as "reported intent X !=
measured effect Y at the same `tool_call_id`".

Today the binding + intent-effect attributes live in a private
`assay.*` namespace because we did not want to squat on
unspecified OTel GenAI / OpenInference attribute names. Three of
them feel like they could be widely useful in any setup that
pairs a tracing layer with an out-of-band runtime-evidence
artifact:

- `agent.runtime_evidence.digest` — content-addressed pointer to
  the artifact (e.g., `sha256:<manifest-bytes>`).
- `agent.runtime_evidence.health` — short enum/string for
  measurement integrity (`clean`, `degraded`, `unknown`).
- `agent.runtime_evidence.boundary` — declared measurement boundary
  string (e.g., `linux_ebpf_cgroup_v2`).

Plus a fourth that emerged from the adversarial scenario:

- `agent.runtime_evidence.intent_effect_status` — short enum/string
  reporting how the trace's reported tool arguments compare to the
  runtime artifact's measured effects (e.g.,
  `intent-effect-match`, `intent-effect-mismatch:<path>`,
  `not-applicable`, `inconclusive`).

The narrow question I'd like maintainer input on:

> Where should attributes like these live? Do they fit under the
> OpenInference spec, under an OTel GenAI extension, in a separate
> namespace altogether, or is there an existing attribute we should
> be using and just missed?

The runtime-evidence artifact itself is intentionally out of scope
for the trace; the trace only needs to refer to it by digest +
short health + boundary fields. Whatever the right home is, we'd
like to move our `assay.*` private names there so other consumers
that pair OTel-family tracing with out-of-band runtime evidence
can use the same vocabulary.

I've attached the full experiment package in the repo we used to
develop this (Linux/eBPF only, internal/experimental). The
v1-findings.md walks through the four slices that produced the
evidence; the per-run Arm C archives + traces + comparator output
are committed under `runs/v1-arm-c/`, `runs/slice2-arm-c/`, and
`runs/slice3-arm-c/`.

- Experiment package: <https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05>
- Plan doc with claim-class taxonomy + threats to validity: [`runner-vs-otel-shape-comparison-2026-05.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-shape-comparison-2026-05.md)
- v1 findings (with Slice 2 + Slice 3 resolutions): [`v1-findings.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/v1-findings.md)
- Comparator source (stdlib Python): [`compare/compare.py`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/compare/compare.py)

Happy to file separate narrower issues if the discussion suggests
they're better tracked there. Not asking for adoption, not asking
for integration; just want to know where the vocabulary should
live so the cross-tool wiring stays clean.

Thanks,
Roel (Rul1an)

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
