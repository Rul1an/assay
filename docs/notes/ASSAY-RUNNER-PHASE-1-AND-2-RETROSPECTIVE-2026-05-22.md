# Assay-Runner Phase 1 + 2 Retrospective

> **Status:** internal engineering retrospective, not a release announcement,
> not a roadmap, not a product page.
> **Date:** 2026-05-22
> **Scope:** what we set out to do across Phase 1 and Phase 2 of the
> Assay-Runner work, and what we actually have standing in the repo today.
> Honest about the gaps.

Phase 1 ran roughly from `2026-05-18` (the candidate memo) to `2026-05-21`
(delegated acceptance). Phase 2 ran from `2026-05-22` (this document is
written on the day the Phase 2D consolidation audit landed) backwards through
2A, 2B, 2C, and 2D as a continuous arc. This note collapses the whole arc
into one read, because the per-phase docs are accurate but they don't tell
the story from outside.

If you came here from
[GitHub Discussion #1329](https://github.com/Rul1an/assay/discussions/1329)
or the
[AgentSight Issue](https://github.com/eunomia-bpf/agentsight/issues/44),
this is the long form. The short form is at the bottom.

## What We Set Out To Do

The seed question, written down on `2026-05-18` in the candidate memo and
sharpened into a spike contract on `2026-05-20`, was:

> Can Assay produce one verifiable measured-run bundle per shim mode, with
> low-ambiguity layer correlation and honest observation health?

That is a narrower question than "can we build an agent runtime", "can we
ship a runner product", or "can we observe agents in production". It is
deliberately about *one run at a time*, *one bundle*, *verifiable*, *honest
about what the bundle does and does not see*.

The bet was that if Assay's evidence model is going to be useful for agent
systems, then somebody had to prove that a measured run could produce
deterministic, low-ambiguity, layer-correlated evidence on real hardware
without lying about gaps. Not in a notebook. Not in a mock. On a kernel,
under a policy, against a real SDK runtime.

Phase 1 was the kill-or-pass for that bet. Phase 2 was everything you have to
do *after* a spike passes to keep the proof reviewable and not let it rot.

## Phase 1 — The Spike That Either Passed Or Got Deleted

Phase 1 was bounded to one platform (Linux + eBPF), one delegated host
(`assay-bpf-runner`), and three proof modes: `kernel-only`,
`kernel+policy`, and `OpenAI Agents kernel+policy+SDK`. Three runs per mode,
plus three-run determinism. No live LLM calls.

It passed on 2026-05-21. Workflow run
[`26211485614`](https://github.com/Rul1an/assay/actions/runs/26211485614),
commit `56571045`, `gates=all`. The delegated job ran for 13 minutes 11
seconds. The acceptance record is
[`ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md`](./ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md);
the byte-level evidence is in
[`docs/reference/runner/proof-packs/phase1-delegated-2026-05-21.md`](../reference/runner/proof-packs/phase1-delegated-2026-05-21.md).

What Phase 1 proved, in plain terms:

- The eBPF program loads and attaches on a real Linux host that we don't own
  and didn't tune for this run.
- The child process can be placed into a clean measured cgroup *before* it
  spawns, so the kernel observation window matches the process's lifetime,
  not a best-effort window around it.
- Kernel observation can complete with `ringbuf_drops=0` — meaning we did not
  lose any events the kernel handed us, and the bundle does not have to lie
  about gaps to look clean.
- Policy decisions made by the Assay MCP wrapper can be correlated to
  kernel-observed side effects.
- SDK tool-call events from the real `@openai/agents` runtime can be
  correlated to policy events by `tool_call_id`, on a deterministic local
  model provider.
- The resulting archive can be verified by the existing Assay evidence path,
  meaning the runner bundle does not need its own verifier.

What Phase 1 explicitly did *not* prove, and what we kept honest about in
the acceptance record:

- It does not prove macOS or Windows attribution.
- It does not prove live LLM execution.
- It does not prove arbitrary SDK compatibility beyond the validated
  `@openai/agents` fixture.
- It does not prove production traffic, sustained load, or a runner fleet.
- It does not prove event-level syscall causality or ordered trace semantics.
- It does not include a ring-buffer drop debug mode (deferred to
  [#1271](https://github.com/Rul1an/assay/issues/1271)).

That list matters. Half of the value of Phase 1 was *not claiming the things
we hadn't tested*.

## Phase 2 — Keeping The Proof Reviewable

Phase 1 produced a proof. Phase 2 had to make sure that proof did not rot
the moment it stopped being the centre of attention. Four sub-phases came
out of that, each with a different concern.

### Phase 2A — Freeze the contracts

The Phase 1 acceptance referenced a set of artifact shapes: observation
health, capability surface, correlation report, archive manifest, SDK events.
Phase 2A pinned those as `assay.runner.*.v0` schemas with frozen field
names, frozen value vocabularies, and a documented strict-vs-degraded
boundary. It also pinned the CI lane contract (which PRs need delegated
proof vs which don't), the acceptance fixture v0 contract, and the boundary
map (`docs/reference/runner/boundary-map.md`).

Nothing got built in 2A that hadn't already passed in 1. The point of 2A was
to make the proof reviewable by someone who wasn't in the room when it
landed.

### Phase 2B — Capability-diff, second runtime

Phase 2A proved the shapes. Phase 2B asked: does anything *use* those shapes
in a way that catches regressions automatically? Two things landed:

- The `assay.runner.capability_surface.v0` artifact got an idempotent
  capability-diff acceptance: run the fixture twice on the same input, the
  diff has to be empty. Non-empty diff = automatic fail.
- A second runtime was added (Gemini via `@google/genai`) so the SDK-layer
  story wasn't load-bearing on `@openai/agents` alone. The Gemini fixture
  qualified under the same idempotent acceptance.

That's when the Runner stopped being "we have a spike that worked once" and
started being "we have a regression boundary that catches drift".

### Phase 2C — Cross-runtime diff

With two runtimes qualifying under the same v0 contracts, the next obvious
question was: *what differs between them, and is that diff stable enough to
be a meaningful comparison surface?* Phase 2C answered that as the
`assay.runner.cross_runtime_diff.v0` contract, with:

- a normative golden shape
  ([`golden/cross-runtime-diff-s5-gemini-v0.json`](../reference/runner/golden/cross-runtime-diff-s5-gemini-v0.json)),
- a JSON Schema 2020-12 sidecar
  ([`schema/cross-runtime-diff-v0-clean.schema.json`](../reference/runner/schema/cross-runtime-diff-v0-clean.schema.json)),
- a documented decision record
  ([`cross-runtime-diff-decisions.md`](../reference/runner/cross-runtime-diff-decisions.md))
  for the A1+B3+C1 choice that locked in canonical output without burying
  the diagnostic projection.

The cross-runtime diff is not a product. It is a regression surface. If a
schema change makes two runtimes diverge unexpectedly, the diff catches it
before it ships.

### Phase 2D — Extraction-readiness without extracting

This is the most counter-intuitive phase, and the one most likely to be
misread by an outsider. The work in 2D was *getting Assay-Runner to a point
where it could leave the Assay repository*, while explicitly **not** leaving
the Assay repository.

The reasoning: deciding to extract a runner is a one-way door. Once you
split, you have two repos, two release cadences, two consumer surfaces, and
you have to pretend you have an external user even if you don't. The
honest version is to do all the boundary work first, prove that Assay
itself can consume the runner as if it were external, observe the boundary
under churn, and only then talk about a real split.

Phase 2D was nine sequential PRs (#1311 through #1325, with #1316 as the
roadmap itself and #1318 as a process drop-in) running against four named
structural blockers:

1. `assay.runner.*.v0` schemas lived inside `crates/assay-runner-spike/src/`,
   making them legally owned by the spike crate.
2. Archive verification crossed an unresolved boundary conflict: manifest
   semantics were schema-shaped, but assembly mechanics were spike-shaped,
   and both sat in `archive.rs`.
3. Cgroup placement depended on `crates/assay-cli/src/cgroup.rs`,
   meaning the spike couldn't run without `assay-cli` internals.
4. No non-spike consumer existed for the runner bundle format. `assay-cli`
   imported through the spike wrapper. Nothing tested the boundary as
   external.

Slices 1, 2, 3, and 6B resolved blockers 1, 2, 3, and 4 respectively.
Slices 4 and 5 moved adjacent boundaries (platform composition; fixture
package boundary) without resolving a named blocker — those landed as
boundary-freeze docs and as `runner-fixtures/` package moves. Slice 6A
opened a design note for what "Assay consumes Runner as external" means,
and Slice 6B made that real: `assay-cli` now depends directly on
`assay-runner-schema`, `assay-runner-core`, and `assay-runner-linux`, with
no `assay_runner_spike::` imports anywhere in `crates/assay-cli/src/`. A
mechanical absence check in `scripts/ci/assay_runner_lane_check.py`
enforces this going forward.

Slice 7 — the actual repository split — was the only slice that did not
land. It is still closed. The audit document that decides when it may open
is
[`phase-2d-consolidation-audit.md`](../reference/runner/phase-2d-consolidation-audit.md),
which is the last piece of work covered by this retrospective.

## What We Have Standing Today

If you `git clone` the repository right now and look at the runner side,
you see this:

```text
crates/assay-runner-schema/   publish=false   schemas + manifest types
crates/assay-runner-core/     publish=false   archive assembly, layer normalizers
crates/assay-runner-linux/    publish=false   cgroup v2 placement primitives
crates/assay-runner-spike/    publish=false   legacy alias wrapper (no live consumer)
runner-fixtures/              package tree    Gemini + OpenAI Agents acceptance fixtures
docs/reference/runner/        15+ contracts   boundary map, CI lanes, fixtures, diffs
scripts/ci/                   classifiers     lane-check + self-test guards
```

All four publish-disabled. The spike crate is a re-export shim retained as a
navigational alias for readers of pre-Slice-6B history; nothing in the
workspace depends on it for production code anymore.

We have:

- A delegated proof on real Linux/eBPF hardware that passes `gates=all`
  including a real `@openai/agents` runtime.
- A second qualified runtime (`@google/genai`) producing idempotent
  capability-diff results under the same v0 contracts.
- A cross-runtime diff with a golden shape, a JSON Schema, and a decision
  record.
- A lane-check classifier that decides which PRs need delegated proof and
  enforces, with a self-test, that no PR re-introduces the spike crate as a
  production consumer.
- A consolidation audit page that defines burn-in criteria instead of a
  passive 4-6 week calendar wait.

We do not have:

- A standalone product.
- A separate repository for Assay-Runner.
- Any published crate from the runner side.
- A macOS or Windows measurement path.
- A live LLM call path (the deterministic local provider is the supported
  path).
- Any concrete external consumer who has said "we need this".
- Any release commitment.
- Any timeline for opening Slice 7.

The gap between "have" and "do not have" is, on purpose, large. Phase 1 and
Phase 2 deliberately stopped short of every line that turns a spike into a
product. The bet is that *not* crossing those lines is what keeps the
evidence honest.

## What We Are Now Doing Instead Of Releasing

Two things, both passive, both gated on external signal:

- A [GitHub Discussion](https://github.com/Rul1an/assay/discussions/1329) on
  this repo asking whether anyone would use a standalone deterministic
  measured-run subsystem, and for what.
- A [maintainer-level sanity-check issue at AgentSight](https://github.com/eunomia-bpf/agentsight/issues/44),
  the system-level eBPF observability project that sits closest to where
  Assay-Runner ended up. The question to them is narrow: is a deterministic
  proof-bundle layer adjacent and useful next to live monitoring, or is it
  duplicative, or is it the wrong abstraction.

We are not running a launch. We are not pinging maintainers. We are not
cross-posting. The probe is intentionally low-pressure.

In parallel, the burn-in criteria in the consolidation audit page are
running: zero of them are observed yet, because at the moment this note is
written we just landed the audit page itself. The first organic
Runner-impacting maintenance PR (a natural candidate is the ring-buffer
diagnostic projection at
[#1271](https://github.com/Rul1an/assay/issues/1271)) will be the first
burn-in evidence point.

## Short Form

If you skipped the rest of this document:

- We asked whether Assay could produce verifiable measured-run bundles on
  real Linux/eBPF hardware, with low-ambiguity layer correlation and honest
  observation health.
- For the delegated Linux/eBPF path, the answer is yes.
- After the proof passed, we did the boundary work needed to make the
  runner extractable, without extracting it. The boundary is now stable
  enough that the only remaining gate is external demand, which we have
  not measured.
- The extraction question stays closed until either a concrete external
  consumer appears or a documented consolidation burn-in is observed.
- There is no release. There is no separate repository. There is no
  promise.

If you have a use case that looks like "a deterministic proof bundle would
fit my CI or release gate", the right place to say so is
[Discussion #1329](https://github.com/Rul1an/assay/discussions/1329). If
not, no harm done; the work was internally useful regardless.

## References

- [Assay-Runner Phase 1 spike plan](./ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md)
- [Assay-Runner Phase 1 acceptance](./ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md)
- [Phase 1 delegated proof pack](../reference/runner/proof-packs/phase1-delegated-2026-05-21.md)
- [Assay-Runner boundary and extraction map](../reference/runner/boundary-map.md)
- [Assay-Runner extraction roadmap (Phase 2D slice sequence)](../reference/runner/extraction-roadmap.md)
- [Runner platform and extraction readiness](../reference/runner/platform-and-extraction-readiness.md)
- [Phase 2D consolidation audit](../reference/runner/phase-2d-consolidation-audit.md)
- [Runner cross-runtime diff v0 contract](../reference/runner/cross-runtime-diff-v0.md)
- [Runner capability-diff v0 contract](../reference/runner/capability-diff-v0.md)
- [Runner CI lane contract](../reference/runner/ci-lanes.md)
- [Runner reference index](../reference/runner/index.md)
