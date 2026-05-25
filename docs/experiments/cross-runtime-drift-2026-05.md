# Cross-Runtime Capability-Surface Drift: Plan

> **Status:** Slices 0–5 drafted. Slices 0–2 are live; Slice 3
> workflow + helpers are ready to dispatch; Slice 4
> [`findings.md`](cross-runtime-drift-2026-05/findings.md) and
> Slice 5 [`publication/`](cross-runtime-drift-2026-05/publication/)
> are draft against synthetic fixtures with explicit substitution
> hooks for live data. The only step left to maintainer-action is
> dispatching the Slice 3 workflow with the
> `OPENAI_API_KEY` + `GOOGLE_API_KEY` secrets, committing the
> resulting `runs/{a0,b0,drift}/` baselines, and substituting the
> synthetic-fixture tables in `findings.md` / `publication/blog-draft.md`
> per the procedure documented in `findings.md`.
> See [`cross-runtime-drift-2026-05/README.md`](cross-runtime-drift-2026-05/README.md)
> for the layout. Companion to
> [`runner-vs-otel-shape-comparison-2026-05.md`](runner-vs-otel-shape-comparison-2026-05.md);
> reuses the same Runner archive + capability_surface contract, so
> the L2 capture machinery is already proven on
> [`assay-bpf-runner`](../../.github/workflows/runner-otel-experiment.yml).
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
runtimes and what stays invariant? Each dimension is pinned to
the **exact archive source** that supplies the data, so the
comparator cannot silently mix layers.

- **Filesystem paths touched** (source: `capability_surface.filesystem_paths`):
  set of paths the runtime accessed. Under v0 this is *undifferentiated*
  (read vs write vs create vs remove are not split). See Threats to
  Validity #5 — the read/write/create/remove split is a deferred
  v2-comparator follow-up that requires parsing `layers/kernel.ndjson`
  directly.
- **Network endpoints** (source: `capability_surface.network`):
  hosts, ports, CIDRs the runtime contacted.
- **Process execs** (source: `capability_surface.processes` /
  exec events in `layers/kernel.ndjson`): child processes the
  runtime spawned (loader, language runtime, sidecars).
- **SDK-layer tool events** (source: `layers/sdk.ndjson`):
  registration + invocation events emitted by the agent SDK via
  `$ASSAY_RUNNER_SDK_EVENT_LOG`. This is the same channel
  Slice 2 of `runner-vs-otel-2026-05` proved end-to-end. Only
  available if the runtime emits SDK events; mark
  `inconclusive` for runtimes that do not.
- **MCP-layer tool surface** (source:
  `capability_surface.mcp_tools`): server/client/tool names
  surfaced by the Runner's policy/MCP layer. This is *not* the
  same as SDK tool registration — it is the policy-side view of
  what the runtime exposed.
- **Tool invocation order**: only available where SDK events
  carry sequence/timestamp info per `tool_call_id`. Mark
  `inconclusive` if the runtime's SDK events do not preserve
  ordering.

The experiment does not ask "which runtime is better." It asks
"what does the runtime choice cost or hide at the
capability_surface level."

## First Conclusion to Test

Two agent runtimes that pass the same functional task (touch
fixture path X, touch fixture path Y, call tool Z) will produce
capability_surfaces that are **not** structurally identical,
even after the workload is held constant. The drift is bounded
and explainable: it falls into runtime-induced (SDK loader,
telemetry sidecar, vendored deps), provider-induced (host
endpoints, auth probes), and task-induced (the paths the agent
actually touched to satisfy the workload contract).

Useful artefact: a per-dimension drift report that names which
slice of the surface is runtime-specific, which is
provider-specific, and which is genuinely task-induced. That
report becomes a runtime-selection input ("this runtime adds N
extra outbound hosts and M extra touched paths purely from its
own machinery") without becoming a benchmark ("runtime X is
faster").

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

## Open Questions

These were the framing choices pulled up front so the plan did
not silently bake in defaults. **Q1 and Q3 are resolved by
Slice 1** (see
[`cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md`](cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md));
Q2, Q4, Q5 remain open.

1. **[RESOLVED — Slice 1]** **Which second runtime, and in which tool-calling mode?**
   - **Option A:** Google Gemini via `@google/genai` (unified SDK,
     newest, supports both manual and automatic function calling).
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
   - **Sub-decision — manual vs automatic function calling.** Slice 1
     must pin which Gemini mode is used. Recommended: **manual**
     function-calling loop, so the wrapper loop is *our* code
     (visible, deterministic, reviewable) instead of the SDK's
     auto-dispatch path (which would itself contribute drift we
     do not control). The OpenAI arm already runs the equivalent
     manual loop via `@openai/agents` — same modality on both
     sides keeps the comparison honest. Pins:
     [Gemini function calling docs](https://ai.google.dev/gemini-api/docs/function-calling),
     [Google Gen AI JS SDK docs](https://googleapis.github.io/js-genai/).

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

3. **[RESOLVED — Slice 1]** **Determinism?**
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

| Dimension | Source | What "drift" looks like |
|---|---|---|
| Filesystem paths touched | `capability_surface.filesystem_paths` | Set difference of paths touched (undifferentiated under v0; see Threats #5) |
| Network hosts | `capability_surface.network` | Set difference of outbound hostnames |
| Network ports/CIDRs | `capability_surface.network` | Set difference of port + CIDR combinations |
| Process execs | `capability_surface.processes` / `layers/kernel.ndjson` | Set difference of exec targets |
| SDK tool events | `layers/sdk.ndjson` | Set difference of SDK-emitted tool registration + invocation events; `inconclusive` for runtimes that emit no SDK events |
| MCP tool surface | `capability_surface.mcp_tools` | Set difference of MCP server/client/tool names surfaced by the policy/MCP layer |
| Tool invocation order | `layers/sdk.ndjson` (sequence/timestamp) | Per-`tool_call_id` ordering diff; `inconclusive` if the runtime's SDK events do not preserve order |

**Out of scope for v0 of this comparator** (explicit follow-ups,
not silent gaps): read-vs-write classification, per-path access
counts, syscall-level kernel.ndjson parsing. Each of these
requires a richer parser than `capability_surface` v0 supplies.
They are tracked alongside the runner-vs-otel
"What still does NOT prove" list and would arrive together as a
v2-comparator follow-up.

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

| Slice | Status | Deliverable | External dependency |
|---|---|---|---|
| 0 | **Done** | This plan-doc, reviewed and sharpened (Open Questions #1 + #3 resolved) | None |
| 1 | **Done** | Workload contract ([`WORKLOAD_CONTRACT.md`](cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md)), `@openai/agents` workload, `@google/genai` manual-function-calling workload, stdlib [`contract-checker`](cross-runtime-drift-2026-05/contract-checker/) with stdlib unit-test coverage of the happy path and each rule failure mode | OpenAI key (already used), Google key for live local testing |
| 2 | **Done** | [`compare/drift.py`](cross-runtime-drift-2026-05/compare/drift.py) MVP + [`health_gate.py`](cross-runtime-drift-2026-05/compare/health_gate.py) + [`extract_fixture_paths.py`](cross-runtime-drift-2026-05/compare/extract_fixture_paths.py) helpers, with stdlib unit-test coverage. Scope-locked to v0 surface: touched-path set diff, network host/port/CIDR diff, process exec diff, SDK tool-event diff, MCP tool-surface diff, tool invocation order. Synthetic fixtures under [`compare/fixtures/{arm-a-openai, arm-b-gemini}/`](cross-runtime-drift-2026-05/compare/fixtures/) exercise every classification label exactly once. Output schema: `assay.cross_runtime_drift.v0`. | None |
| 3 | **Workflow ready** | Live Arm A0 + B0 dispatch on `assay-bpf-runner` via [`.github/workflows/cross-runtime-drift-experiment.yml`](../../.github/workflows/cross-runtime-drift-experiment.yml) (workflow_dispatch only). Three jobs: `arm-a-openai`, `arm-b-gemini`, `drift-compare`. Awaits maintainer dispatch with `GOOGLE_API_KEY` secret added; baselines then committed under `runs/{a0,b0}/` per [`runs/README.md`](cross-runtime-drift-2026-05/runs/README.md). | `OPENAI_API_KEY` (already set) + `GOOGLE_API_KEY` as repo secrets |
| 4 | **Drafted (synthetic-fixture baseline)** | [`findings.md`](cross-runtime-drift-2026-05/findings.md) — drift table, classification, threats-to-validity, reproduction commands. Explicit substitution procedure for the live-data tables once Slice 3 baselines are committed. | None |
| 5 | **Drafted (not filed, not published)** | [`publication/`](cross-runtime-drift-2026-05/publication/) — `README.md` sequencing rules, `blog-draft.md` engineering write-up, `discussion-draft.md` for the **comment** on #3162 (not a new issue). Held until live baselines land and OpenInference #3162 sees triage. | OpenInference #3162 triage outcome |

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
