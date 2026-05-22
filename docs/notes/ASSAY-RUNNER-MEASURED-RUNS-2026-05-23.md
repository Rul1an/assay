# Measured Runs Are Not Traces

> **Status:** internal engineering note, not a release announcement.
> **Date:** 2026-05-23
> **Scope:** the conceptual distinction between traces/observability and a
> deterministic measured-run proof bundle; companion to the
> [read-only walkthrough](../reference/runner/examples/measured-run-proof-bundle.md)
> and the [Phase 1 + 2 retrospective](./ASSAY-RUNNER-PHASE-1-AND-2-RETROSPECTIVE-2026-05-22.md).

This note exists because two questions keep coming back from people who
look at Assay-Runner for the first time:

> "Is this a tracing tool?"

and

> "Is this an observability dashboard?"

Both reasonable, both no. This is an attempt to explain *why* no, in
plain terms, without pitching a product.

## The Gap Traces Don't Cover

Traces tell you what the application *thought* happened. Span hierarchies,
attributes, status codes, sometimes events. That is genuinely useful and
not something Assay-Runner is trying to replace.

But traces sit at the application layer. They tell you what the agent
*said* it did. They do not tell you what the process *actually* did at
the kernel boundary — which files it opened, which sockets it touched,
which child processes it spawned, whether the policy decision the agent
believed in was the same decision the host system saw.

For most production observability work, that gap is fine. You look at
spans, you fix what's slow, you ship. Nobody needs kernel-grounded
evidence to debug a latency regression.

For agent systems in CI, release-gate, or audit contexts, the gap
matters more. The question shifts from *"is the agent fast"* to *"is
the agent doing what the policy said it was allowed to do, and can I
prove it deterministically two weeks from now when this release ships"*.
Traces don't carry that proof.

## What A Measured Run Adds

A measured run is one execution of one agent fixture, observed at three
layers simultaneously and bound together by a stable `tool_call_id`:

- **Kernel layer.** cgroup-scoped eBPF events: file opens, network
  endpoints, process execs. What the host actually saw.
- **Policy layer.** MCP allow/deny decisions: what the policy gate
  decided about each tool call.
- **SDK layer.** Normalized agent tool-call events: what the SDK
  claimed it did.

The run produces one deterministic `.tar.gz` archive containing three
load-bearing v0 JSON artifacts (`observation-health.json`,
`capability-surface.json`, `correlation-report.json`), an archive
manifest (`manifest.json` with file digests), a whole-archive event
stream (`events.ndjson`), and three per-layer event streams
(`layers/kernel.ndjson`, `layers/policy.ndjson`, `layers/sdk.ndjson`).
Schemas are frozen as `assay.runner.*.v0`. The archive verifies through
the existing Assay evidence path; runner bundles do not have their own
verifier.

The most load-bearing artifact is **observation health**. It is the
contract that the bundle is *not lying about gaps*. If the kernel
ring-buffer dropped events, the bundle has to say so and degrade. If
policy capture was missing, the bundle has to say so. If the SDK layer
was self-reported only and not kernel-corroborated, it is recorded as
`self_reported` — never as if the SDK had been independently verified.

The [walkthrough](../reference/runner/examples/measured-run-proof-bundle.md)
shows what each artifact looks like in practice, using existing
checked-in golden artifacts from the OpenAI Agents and Gemini
fixtures.

## How This Differs From An Observability Dashboard

An observability dashboard answers:

> What is happening across many runs, right now?

It optimizes for live signal, aggregation, drill-down, and human eyes
in the loop. The system gets faster and louder; the user notices.

A measured-run proof bundle answers:

> For this one run, exactly what did the kernel/policy/SDK layers
> observe, and do those layers correlate cleanly under a frozen
> contract?

It optimizes for byte-stable artifacts, no inferred ordering, explicit
honesty about degraded layers, and CI gates in the loop. The artifact
is the deliverable; it has to survive being diffed two months from now
against today's baseline.

Both are useful. They are not substitutes. A team running an
observability stack might still want a measured-run bundle for the
narrow case where they need to *prove* what one specific release did
under a specific policy. The reverse — running measured-run bundles
where you actually want live monitoring — would be the wrong shape.

## Why Standalone Extraction Is Not Decided

The four structural blockers that would have prevented Assay-Runner
from leaving the Assay repository have been resolved as of Phase 2D
Slice 6B. The schemas live in their own crate, archive assembly lives
in its own crate, cgroup placement lives in its own crate, and Assay
itself consumes the runner via those crates rather than through the
spike wrapper.

That makes extraction *possible*. It does not make extraction *right*.

The honest version is that no concrete external consumer has appeared.
We did not build this because someone asked for a deterministic
proof-bundle subsystem; we built it because Assay needed measured-run
evidence and the boundary work was good engineering hygiene. The
question of whether it should ever leave the Assay repo is downstream
of "would anyone use it standalone", and that is not a question we can
answer from inside the repo.

The
[Phase 2D consolidation audit](../reference/runner/phase-2d-consolidation-audit.md)
replaces the original "wait 4–6 weeks" rule with explicit burn-in
criteria: at least two normal PRs through the new boundary, at least
one Runner-impacting maintenance PR through the per-PR discipline, no
re-introduction of the spike crate, no public API widening for routine
fixes. None of these criteria are observed yet at the time of writing
because the audit just landed.

So: extraction is technically unblocked, evidentially unproven, and
externally unmotivated. The right answer in that situation is "wait
and look", not "split".

## The Honest Question To External Maintainers

If you are maintaining an agent-observability project — system-level
or application-level — the question this work is trying to answer is
narrower than "would you use Assay-Runner?". It is:

> Would a deterministic measured-run proof-bundle layer be useful as a
> CI or release companion next to what you already do?

Three answers all give us useful signal:

- **Yes, this would slot in next to our live monitoring as a CI
  evidence artifact.** That is a real external use case; the audit
  page would treat it as one of the triggers for opening the
  extraction question.
- **No, our users want streaming/live signal, not bundles.** That is a
  clean shape-mismatch verdict; it goes into the consolidation audit
  as evidence that the abstraction is wrong for that audience, and
  the extraction question stays closed.
- **Maybe, if it emitted OTel-shaped output.** That is a bridge
  question, not a Slice 7 trigger, but it tells us where the future
  contract surface might need to grow.

The places that question is currently being asked, passively:

- [Discussion #1329](https://github.com/Rul1an/assay/discussions/1329)
  on this repo.
- [Issue #44](https://github.com/eunomia-bpf/agentsight/issues/44) at
  AgentSight, as a maintainer-level sanity check.

No pings, no cross-posts, no @-mentions. The probe is intentionally
low-pressure.

## Short Form

- Traces tell you what the agent thought it did.
- A measured run tells you what the kernel saw, what the policy
  decided, and what the SDK claimed — bound together by one stable
  `tool_call_id`.
- One archive per run, five frozen JSON artifacts, two ndjson layer
  streams, verifiable through the existing Assay evidence path.
- This is not an observability dashboard. It is a deterministic
  evidence layer for CI/release/audit contexts.
- Whether it should ever leave the Assay repo depends on someone
  outside the repo saying it would help them. Until then, it stays in.

## References

- [Measured-run proof-bundle walkthrough](../reference/runner/examples/measured-run-proof-bundle.md)
- [Phase 1 + 2 retrospective](./ASSAY-RUNNER-PHASE-1-AND-2-RETROSPECTIVE-2026-05-22.md)
- [Assay-Runner reference index](../reference/runner/index.md)
- [Phase 2D consolidation audit](../reference/runner/phase-2d-consolidation-audit.md)
- [Runner artifact v0 contracts](../reference/runner/artifacts-v0.md)
- [Runner cross-runtime diff v0 contract](../reference/runner/cross-runtime-diff-v0.md)
