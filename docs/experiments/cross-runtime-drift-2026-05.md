# Cross-Runtime Capability-Surface Drift: Plan

> **Status:** **draft, no code written yet.** Plan-doc only, lives in
> the repo so it can be sharpened the same way
> [`runner-vs-otel-shape-comparison-2026-05.md`](runner-vs-otel-shape-comparison-2026-05.md)
> was iterated before Slice 1 landed. Companion to that experiment;
> reuses the same Runner archive + capability_surface contract, so
> the L2 capture machinery is already proven on
> [`assay-bpf-runner`](.github/workflows/runner-otel-experiment.yml).
>
> **Last updated:** 2026-05-25
>
> **Scope:** measure the structural drift in the Runner-captured
> capability_surface when the *same agent task* is executed by two
> different agent runtimes. Not a benchmark, not a quality ranking,
> not a model comparison.

## Research Question

When the same agent task is executed by two different agent
runtimes under a single Runner capture boundary, where does the
measured capability_surface differ structurally?

Concretely, for each of these dimensions, what changes between
runtimes and what stays invariant?

- **Filesystem**: which paths are read, written, created, removed
- **Network**: which hosts, ports, CIDRs are contacted
- **Process**: which child processes / execs occur (loader,
  language runtime, sidecars)
- **Tool registration**: which tools the SDK advertises to the
  model
- **Tool invocation**: which tools the model actually called, in
  what order
- **MCP layer**: which MCP servers/clients appear in
  `capability_surface.mcp_tools`
- **SDK layer**: which schema and event shapes appear in
  `layers/sdk.ndjson`

The experiment does not ask "which runtime is better." It asks
"what does the runtime choice cost or hide at the
capability_surface level."

## First Conclusion to Test

Two agent runtimes that pass the same functional task (read file
X, write file Y, call tool Z) will produce capability_surfaces
that are **not** structurally identical, even after the workload
is held constant. The drift is bounded and explainable: it falls
into runtime-induced (SDK loader, telemetry sidecar, vendored
deps), provider-induced (host endpoints, auth probes), and
task-induced (the actual reads/writes the agent decided to do).

Useful artefact: a per-dimension drift report that names which
slice of the surface is runtime-specific, which is
provider-specific, and which is genuinely task-induced. That
report becomes a runtime-selection input ("this runtime adds N
extra outbound hosts and M extra writes purely from its own
machinery") without becoming a benchmark ("runtime X is faster").

## SOTA Anchors

This experiment sits on top of three things that already exist:

| Layer | Provided by | Status |
|---|---|---|
| L2 Runner capture | `assay runner-spike` + cgroup-v2 + eBPF | Stable, proven on slices 1-3 of the runner-vs-otel-2026-05 experiment |
| Capability_surface v0 schema | `assay-runner-schema` crate | Stable, used by [`v1-findings.md`](runner-vs-otel-2026-05/v1-findings.md) |
| Agent runtimes under comparison | OpenAI Agents SDK (Node) and one other | OpenAI side proven; second runtime TBD — see Open Questions |

The drift report is a **new** comparator. It does **not** reuse
[`runner-vs-otel-2026-05/compare/compare.py`](runner-vs-otel-2026-05/compare/compare.py),
which is a trace-vs-archive comparator. A new
`compare/drift.py` is the artefact.

## Open Questions (must resolve before Slice 1)

These are the framing choices I want sharpened before any code
lands. Pulling them up front so the plan does not silently bake
in defaults.

1. **Which second runtime?**
   - **Option A:** Google Gemini via `@google/genai` (unified SDK,
     newest, has agent-style tool calling).
   - **Option B:** Google Gemini via `@google/generative-ai` (older
     official SDK, narrower surface).
   - **Option C:** Anthropic Claude via `@anthropic-ai/sdk` with
     a thin agent loop (no first-party agent SDK at the time of
     writing, so the loop becomes part of the surface we measure
     — which is itself interesting, but conflates runtime + loop).
   - **Option D:** LangChain JS as the second runtime, with
     OpenAI as the underlying model. Tests "drift from agent
     framework choice" rather than "drift from provider choice."
   - **Recommended starting point:** Option A. Closest semantic
     parity with `@openai/agents`, single-vendor stack so we are
     measuring runtime drift not also-framework drift.

2. **Same workload semantics or same workload code?**
   - **Same code** is impossible: the two SDKs register tools
     differently.
   - **Same semantics** means: same prompt intent, same logical
     tools (read_file, write_file), same expected effect (read
     X, write Y). The workload is ported, not copied.
   - **Recommended:** same semantics, with a written-down
     "workload contract" the two implementations must satisfy.
     Then the drift is the *runtime overhead* of the same
     contract, not "two different programs."

3. **Determinism?**
   - OpenAI Agents has `temperature: 0` + structured output
     mode; we already exercise this in the runner-vs-otel
     workload's `--deterministic-fixture` path.
   - Gemini supports `temperature: 0` but does not expose a
     proper deterministic mode.
   - Anthropic supports `temperature: 0` only.
   - **Recommended:** sidestep the model entirely by going
     "tool-calling-only with a deterministic synthetic prompt
     that forces a specific tool sequence." We do not need the
     model to be smart; we need the *runtime* to dispatch
     known tool calls. Mirrors the runner-vs-otel deterministic
     fixture pattern.

4. **N per arm?**
   - Slice 1-3 of runner-vs-otel used n=3 for shape stability.
   - Drift comparison is also a shape claim, not a latency
     claim, so n=3 is sufficient *for the drift report itself*.
   - **Recommended:** n=3 per arm for the first findings doc;
     bump to n>=5 only if drift turns out to be noisy across
     runs of the same runtime.

5. **Provider credentials in CI?**
   - OpenAI key already configured for runner-vs-otel.
   - Gemini key would be new. Decision: do we add it as a
     workflow secret, or do we keep this experiment local-only
     until the plan is approved?
   - **Recommended:** local-only for the proof-of-shape Slice 1,
     then add the secret + dispatch on `assay-bpf-runner` for
     Slice 2.

## Experimental Arms

| Arm | Runtime | Provider | Capture | Purpose |
|---|---|---|---|---|
| A0 | `@openai/agents` Node SDK | OpenAI | Runner Arm C (eBPF + sdk-event-log) | Baseline; already covered by runner-vs-otel Slice 1-3 evidence |
| B0 | TBD (Option A recommended: `@google/genai`) | TBD | Runner Arm C | The drift target |

Both arms run **the same workload contract** under the same
Runner boundary, capture, and host. The only deliberately
varied input is the runtime + provider combination.

## Workload Contract

The workload contract is a short written spec, not a code blob.
Slice 1 produces it. Suggested first draft:

> The workload registers two tools: `read_file(path: string) ->
> string` and `write_file(path: string, contents: string) ->
> void`. The agent is prompted to "read FIXTURE_INPUT_PATH,
> rewrite the contents in uppercase, write to
> FIXTURE_OUTPUT_PATH". Tool calls must occur in order: read,
> then write. Workload exits 0 on success.

Each runtime implementation must satisfy the contract. Any
deviation is a workload bug, not a drift signal, and must be
fixed before the run is counted.

## Drift Dimensions and Comparator Output

The `compare/drift.py` comparator takes **two Runner archives**
(one per arm) and emits per-dimension drift:

| Dimension | What "drift" looks like |
|---|---|
| Filesystem reads | Set difference of paths read by each runtime |
| Filesystem writes | Set difference of paths written |
| Filesystem creates/removes | Set difference of paths touched |
| Network hosts | Set difference of outbound hostnames |
| Network ports/CIDRs | Set difference of port + CIDR combinations |
| Process execs | Set difference of `exec` syscall targets |
| Registered tools | Set difference of MCP/agent tool names |
| Invoked tools | Set difference of actually-called tool names |
| SDK layer schema | Schema string + event-shape diff |

Each row carries a **classification label**:

- `runtime-induced`: present in one runtime's surface and absent
  in the other, attributable to runtime/loader/sidecar machinery.
- `provider-induced`: attributable to the model provider's auth
  or transport.
- `task-induced`: shows up in both arms because the contract
  requires it.
- `inconclusive`: cannot classify automatically; needs human
  review.

The comparator outputs `drift.json` + `drift.md` per pair of
archives. Acceptance criterion below.

## Threats to Validity

1. **"Same contract" is a manual judgement.** If the two
   workload implementations subtly do different things, the
   drift report blames the runtime when it should blame the
   workload. Mitigation: workload-contract checks (tools
   called in correct order, exit code 0, expected output file
   exists with expected content). Any contract violation
   invalidates the run.

2. **Provider auth probes are not the runtime's fault.** A
   `HEAD https://auth.openai.com/v1/...` is "runtime-induced"
   in the SDK's choice to make the call but "provider-induced"
   in the *target* of the call. Comparator must mark this
   honestly, not collapse the two categories.

3. **Single-host bias.** All captures run on the same
   `assay-bpf-runner` VM. Kernel-specific quirks are constant
   across arms (good — controls for kernel), but the result is
   not portable to other distros without re-running.

4. **One snapshot in time.** SDK versions move fast. The
   plan-doc must pin both runtimes by package version + git SHA
   and re-run if either bumps a major.

5. **Capability_surface v0 granularity.** v0 records "what
   paths were touched" but not "how many times" or "in what
   order." Slice 4 of runner-vs-otel already flagged this as a
   v2-comparator follow-up. Same caveat applies here: drift on
   *what* the runtime touches is in scope; drift on *how often*
   is not.

6. **The drift report is not a security claim.** "Runtime B
   contacts an extra host" is a runtime-selection input, not a
   "runtime B is insecure" claim. Findings doc must repeat this
   up front.

## Acceptance Criteria

A successful first findings doc:

1. Workload contract is written, both implementations satisfy
   it, both arms exit 0.
2. n=3 archives per arm captured under Runner Arm C with
   `observation_health.ringbuf_drops = 0` and clean
   `cgroup_correlation`.
3. `drift.py` runs on each (A0, B0) archive pair and produces
   `drift.json` + `drift.md` for n=3 pairs.
4. Each drift row classified as runtime-induced /
   provider-induced / task-induced / inconclusive.
5. The findings doc explains every `inconclusive` row by hand
   or downgrades it to a known limitation.
6. Threats to Validity section is repeated verbatim from this
   plan and updated with anything we learned during execution.

## Sequencing (slices)

| Slice | Deliverable | External dependency |
|---|---|---|
| 0 | This plan-doc, reviewed and sharpened (Open Questions resolved) | None |
| 1 | Workload contract written down. Both runtime implementations of the workload, runnable locally with API keys. Each passes the contract checker. | OpenAI key (already used), second runtime's key |
| 2 | `compare/drift.py` MVP. Runs on two **synthetic** archives derived from existing runner-vs-otel fixtures, no live runs yet. Comparator output schema locked in. | None |
| 3 | Live Arm A0 + B0 dispatch on `assay-bpf-runner` (workflow extension). n=3 per arm. Baselines committed under `docs/experiments/cross-runtime-drift-2026-05/runs/{a0,b0}/`. | Second runtime API key as workflow secret |
| 4 | `findings.md`. Drift table, classification, threats-to-validity, reproduction commands. | None |
| 5 | Publication artefacts (issue + blog draft) following the same discipline as runner-vs-otel Slice 4: one narrow question per channel, no maintainer @-mentions, evidence link once. | OpenInference #3162 triage outcome should inform whether to file under same umbrella or separately |

## What this experiment deliberately does NOT do

- Does not benchmark latency or cost. n=3 is a shape claim, not
  a perf claim.
- Does not compare model output quality between runtimes. The
  deterministic synthetic prompt makes the model's role
  minimal; we measure the runtime's machinery, not its brain.
- Does not declare a winner. "Runtime A is simpler/safer/etc"
  is out of scope; the drift report is descriptive only.
- Does not propose new capability_surface schema fields. v0 is
  the contract; v2 granularity (counts, ordering) is a separate
  follow-up tracked in the runner-vs-otel-2026-05 findings.
- Does not @-mention runtime maintainers, file issues on their
  repos, or DM anyone. Publication discipline mirrors Slice 4
  of runner-vs-otel.

## Relationship to runner-vs-otel-2026-05

This experiment **does not invalidate** the OpenInference
question filed as
[`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162).
The four candidate attributes (digest, health, boundary,
intent_effect_status) are runtime-agnostic by design. If
anything, drift evidence across runtimes strengthens the case
for those attributes living somewhere shared.

If OpenInference triage routes us to OTel semconv, the same
attributes work for both runtimes captured here.

## Next step

Decide the Open Questions (especially #1 and #3), then Slice 1.
No code lands until those are pinned. This file is the place to
record the decisions when they are made.
