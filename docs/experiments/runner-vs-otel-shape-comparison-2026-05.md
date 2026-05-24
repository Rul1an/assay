# Runner Archives Next to OTel Traces: Shape Comparison Plan

> **Status:** experiment skeleton; no data collected yet
>
> **Last updated:** 2026-05-24
>
> **Scope:** compare an Assay-Runner measured-run archive with an
> OpenTelemetry-family trace for the same deterministic agent workload. This is
> not a product launch, benchmark claim, or assertion that measured-run archives
> replace traces.

## Research Question

What claims about an agent run can be supported by:

- an OpenTelemetry-family trace using GenAI / OpenInference-style semantic
  attributes;
- an Assay-Runner measured-run archive;
- both artifacts joined together; or
- neither artifact?

The experiment compares evidence shape, joinability, and claim strength. It
does not rank tracing tools, evaluate model quality, or claim semantic
equivalence between runtimes.

## First Conclusion to Test

Traces explain the agent's reported control flow. Measured-run archives bound
the system effects and measurement health. The two are complementary, and the
join key determines whether they become one evidence story or two parallel
observability stories.

On Linux with healthy eBPF capture (`ringbuf_drops=0`, clean cgroup
correlation), a measured-run archive can support bounded negatives: claims of
the form "within this measurement boundary, X did not happen." Traces support
positive observations and intent reconstruction. Together they form a
defense-in-depth observability stack, not competitors.

## SOTA Anchors

This plan intentionally separates three layers that are often collapsed in
AI-observability writing:

| Layer | Measures | Examples | In Scope for v1 |
|---|---|---|---|
| L1 - Reported control flow | What the agent/framework reports | OpenTelemetry GenAI semantic conventions, OpenInference, traceAI, Phoenix, Langfuse, Logfire | Yes |
| L2 - Agent-aware runtime evidence | What the run did, joined to agent context | Assay-Runner, AgentSight-style eBPF + agent correlation | Yes |
| L3 - Generic kernel observability | What the system did without agent context | Tetragon, Falco, Tracee | No, explicit follow-up |

v1 compares L1 and L2. L3 is deliberately out of scope so the first run can be
completed with the existing Assay-Runner fixtures. A follow-up should add one
generic kernel tracer to show what Assay-Runner adds beyond raw system-event
visibility: cgroup correlation, `run_id`, SDK layer correlation, archive
manifest integrity, and measurement-health gates.

## Source Pinning

Before publishing results, pin exact versions or commit SHAs for these moving
inputs:

| Source | Pin Required | Notes |
|---|---|---|
| OpenTelemetry GenAI semantic conventions | Exact docs date and preferably `open-telemetry/semantic-conventions` commit SHA | The GenAI conventions are still marked Development; attribute names may move. |
| OpenInference semantic conventions | Package version and docs URL/commit | OpenInference is chosen as the semantic layer, not as the only possible instrumentation framework. |
| Assay | Git commit and release tag | Record both the source commit and the Runner archive schema strings. |
| Assay-Harness | Version if used for projection | Record `verify-runner`, `runner compare`, and `runner cross-runtime` versions separately. |
| arXiv or research references | Verified URL and title only | Do not cite unverified IDs. Remove any synthetic-looking survey references. |

## Why OpenInference for v1

OpenInference is chosen for v1 because it is a semantic convention and
instrumentation family built on OpenTelemetry, not merely a dashboard. That
lets the experiment separate "what do we call this field?" from "which tool
auto-captured this content?" traceAI, Phoenix, Langfuse, and Logfire remain
important comparison points, but v1 should pin the vocabulary first and avoid
turning the experiment into a product bake-off.

## Experimental Arms

Use the same deterministic agent workload in all arms.

| Arm | Runner archive | OTel / OpenInference trace | Purpose |
|---|---:|---:|---|
| A - Runner only | Yes | No | Establish measured-run baseline and archive determinism. |
| B - Trace only | No | Yes | Establish trace shape without eBPF/cgroup capture. |
| C - Dual capture | Yes | Yes | Main comparison arm; both artifacts share `run_id` and tool-call join keys. |

v1 should run arm C at least three times for shape stability and archive
determinism. Overhead measurements need a larger sample; see
[Measurement Plan](#measurement-plan).

## Workload

Start with one deterministic OpenAI Agents fixture or equivalent local/cassette
provider workload:

- one agent invocation;
- one model/provider call;
- one tool call;
- one policy decision;
- one safe workdir filesystem read/write;
- no live network dependency unless explicitly part of the scenario;
- no streaming response in v1; a streaming variant is deferred so the v1 field
  matrix can be reasoned about against a single complete-message shape.

Gemini and cross-runtime comparison stay out of v1. They can be added after the
single-runtime shape comparison is stable.

## v1.5 Adversarial Scenario

Do not let this disappear into a vague follow-up. Plan it as v1.5:

| Scenario | Reported L1 behavior | Measured L2 behavior | Why it matters |
|---|---|---|---|
| Tool-call argument tampering | `gen_ai.tool.name = fs.read`, `gen_ai.tool.call.arguments.path = /workdir/safe.txt` | kernel layer observes open/read outside the reported path, such as a normalized traversal to `/etc/passwd` or a controlled test file outside workdir | Demonstrates L1 intent/reporting can diverge from L2 effects. |

Use a safe fixture path rather than a real sensitive file if possible. The
claim is not "the agent stole secrets"; the claim is "reported tool arguments
and measured filesystem effects can diverge."

## Trace Attribute Plan

Use exact OpenTelemetry GenAI attributes where available, and add Assay-specific
attributes under an `assay.*` namespace. Keep sensitive content opt-in.

| Field | OTel / OpenInference Attribute | Assay Archive Field | Claim Class |
|---|---|---|---|
| Run identity | `assay.run.id` | `manifest.run_id` or artifact run id | Correlation |
| Provider | `gen_ai.provider.name` | SDK metadata side-band if present | Provenance |
| Operation | `gen_ai.operation.name` | SDK/policy event type if present | Reported control flow |
| Request model | `gen_ai.request.model` | SDK metadata side-band if present | Provenance |
| Response model | `gen_ai.response.model` | SDK metadata side-band if present | Provenance |
| Input tokens | `gen_ai.usage.input_tokens` | Not expected | Cost/context |
| Output tokens | `gen_ai.usage.output_tokens` | Not expected | Cost/context |
| Tool name | `gen_ai.tool.name` | `sdk_event.tool`, `capability_surface.mcp_tools` | Joinable behavior |
| Tool call id | `gen_ai.tool.call.id` if available; otherwise custom fallback | `tool_call_id` where present | Primary join key |
| Tool arguments | opt-in only, e.g. `gen_ai.tool.call.arguments` or OpenInference message/tool-call fields | Policy/SDK layer only if explicitly emitted | Sensitive reported intent |
| Tool result | opt-in only | SDK layer only if explicitly emitted | Sensitive result |
| Filesystem paths | Not expected in trace | `capability_surface.filesystem_paths`, kernel layer | Measured effect |
| Network endpoints | Maybe app span attrs; not guaranteed | `capability_surface.network_endpoints`, kernel layer | Measured effect |
| Process execs | Not expected in trace | `capability_surface.process_execs`, kernel layer | Measured effect |
| Policy decision | Custom span attr or event if instrumented | policy layer, `capability_surface.policy_decisions` | Enforcement |
| Ring buffer drops | Not expected | `observation_health.ringbuf_drops` | Measurement integrity |
| Cgroup correlation | Not expected | `correlation_report.status`, cgroup correlation field | Measurement integrity |
| Archive binding | `assay.archive.manifest_digest` | SHA-256 digest of `manifest.json` bytes | Tamper-evident link |

## Join Hierarchy

Try join keys in this order:

1. `assay.run.id` for run-level binding.
2. `gen_ai.tool.call.id` to Runner `tool_call_id` for tool-level binding.
3. OpenInference message/tool-call id fields if they differ from OTel naming.
4. Tool name + monotonic order within run, marked as weak join.
5. Timestamp proximity, marked as diagnostic only and never used for a strong
   claim.

Measure how often `gen_ai.tool.call.id` is present for each instrumentation
path. This is a data point, not just a prerequisite.

| Instrumentation | `gen_ai.tool.call.id` Present? | Join Grade | Notes |
|---|---|---|---|
| Direct manual OTel SDK | Expected yes by construction | Primary | Used as control. |
| OpenInference OpenAI instrumentation | To measure | Primary or secondary | Record package version. |
| OpenInference LangChain instrumentation | To measure | Primary or secondary | Optional for v1. |
| traceAI auto-instrumentation | To measure if included | Primary or secondary | Optional comparison, not v1 baseline. |

## Manifest Digest Binding

The dual-capture arm should make the trace carry a tamper-evident reference to
the measured-run archive. Add an event or span attributes after the Runner
archive is written:

```text
assay.archive.schema = "assay.runner.archive_manifest.v0"
assay.archive.manifest_digest = "sha256:<manifest-json-bytes>"
assay.archive.path = "<local artifact path or basename>"
assay.runner.kernel_layer = "complete"
assay.runner.ringbuf_drops = 0
assay.runner.correlation_status = "clean"
```

This mirrors the provenance pattern used in supply-chain attestations: a small
structured record refers to an artifact by digest rather than embedding the
artifact itself. It does not make the trace authoritative for archive content;
it lets a consumer verify that the trace and archive are the pair claimed by
the experiment.

### Namespace separation: private vs proposed

Two namespaces deliberately coexist in this experiment:

- `assay.*` is the **private experiment namespace** used by the implementation
  today (`assay.archive.manifest_digest`, `assay.run.id`,
  `assay.runner.correlation_status`, ...). These names are stable for the
  duration of the experiment but make no claim on the open vocabulary.
- `agent.runtime_evidence.*` is the **proposed open-standard namespace**
  documented in the OpenInference Discussion Payload section below
  (`agent.runtime_evidence.digest`, `agent.runtime_evidence.health`,
  `agent.runtime_evidence.boundary`). These are what we will offer up for
  inclusion in the OTel GenAI / OpenInference vocabulary.

The experiment intentionally emits `assay.*` attributes today. If and when the
vocabulary discussion settles on neutral names, the instrumentation can be
re-pointed at the agreed namespace without changing the comparator schema.

### Implementation package

The skeleton in this doc is now backed by a runnable experiment package under
[`runner-vs-otel-2026-05/`](runner-vs-otel-2026-05/README.md):

- `compare/compare.py` (stdlib only) is the field-matrix generator and lock
  the comparator's wire format against the schemas in
  `crates/assay-runner-schema`. It reads either a `.tar.gz` archive or an
  extracted directory and an OTLP/JSON trace, and emits both a JSON and a
  Markdown matrix.
- `workload/` is the Node.js + TypeScript runtime that wraps the existing
  `runner-fixtures/openai-agents/fixture-agent.js` workload with OTel
  tracing and the `assay.archive.created` event binding.
- `run-arm-b.sh` is the local trace-only orchestrator. Arms A and C run on
  the delegated `assay-bpf-runner` host; see the experiment-package
  `README.md` for the dispatch path.

### Comparator exit codes

`compare/compare.py` ships a stable exit-code contract so it can land in CI
or Harness flows without re-inventing semantics:

| Code | Meaning |
|---:|---|
| `0` | Comparison generated; no binding error required or satisfied |
| `2` | Bad CLI / config / input path |
| `3` | Malformed archive or trace, or `--require-binding-match` was set and the manifest digest did not match |

Arm C dispatches set `--require-binding-match`; Arm A, Arm B, and unit tests
do not, so a synthetic or absent counterpart does not cause a hard failure.

### TypeScript Sketch

```ts
import { trace, SpanStatusCode } from "@opentelemetry/api";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

function sha256Digest(bytes: Buffer): string {
  return `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
}

const tracer = trace.getTracer("assay-runner-otel-experiment");

await tracer.startActiveSpan("assay.runner.measured_run", async (span) => {
  try {
    span.setAttributes({
      "assay.run.id": runId,
      "assay.measurement.boundary": "linux_ebpf_cgroup_v2",
      "gen_ai.provider.name": "openai",
      "gen_ai.operation.name": "create_agent",
    });

    await runAgentWorkload();

    const archivePath = await writeRunnerArchive();
    const manifestBytes = readFileSync(extractedManifestPath);
    const manifestDigest = sha256Digest(manifestBytes);

    span.addEvent("assay.archive.created", {
      "assay.archive.schema": "assay.runner.archive_manifest.v0",
      "assay.archive.manifest_digest": manifestDigest,
      "assay.archive.path": archivePath,
      "assay.runner.kernel_layer": "complete",
      "assay.runner.ringbuf_drops": 0,
      "assay.runner.correlation_status": "clean",
    });

    span.setStatus({ code: SpanStatusCode.OK });
  } catch (error) {
    span.recordException(error as Error);
    span.setStatus({ code: SpanStatusCode.ERROR });
    throw error;
  } finally {
    span.end();
  }
});
```

Tool spans should carry both OTel GenAI attributes and the Assay run id:

```ts
await tracer.startActiveSpan("execute_tool mcp_file_read", async (span) => {
  span.setAttributes({
    "gen_ai.operation.name": "execute_tool",
    "gen_ai.tool.name": "mcp_file_read",
    "gen_ai.tool.type": "function",
    "gen_ai.tool.call.id": toolCallId,
    "assay.run.id": runId,
  });

  try {
    return await callTool();
  } finally {
    span.end();
  }
});
```

## Sensitive Content Policy

Do not capture prompt text, completion text, tool arguments, or tool results by
default. Put them behind a named opt-in flag:

```text
--capture-sensitive-otel-content
```

When disabled, the matrix should still compare identity, shape, tool names,
token counts, measured effects, and measurement-health fields. Absence of
prompt/completion content in a trace is a configuration fact, not evidence that
OpenTelemetry cannot carry that content.

## Measurement Plan

| Metric | Purpose | Sample Size | Gate |
|---|---|---:|---|
| Archive determinism | Verify measured-run archive stability | n = 3 | Hard: manifest digests identical for deterministic workload, unless declared non-deterministic field exists |
| Trace shape stability | Verify span tree and attribute keys are stable | n = 3 | Soft: timestamps/durations may vary |
| `gen_ai.tool.call.id` presence | Measure joinability in practice | n = 3 per instrumentation path | Report as primary/secondary/no join |
| End-to-end wall clock | Capture observability overhead | n >= 20 per arm | Report median, p95, p99, p99/median |
| Peak RSS | Capture memory overhead | n >= 5 per arm | Report median and max |
| Archive size | Storage footprint for L2 | n = 3 | Report bytes and compressed size |
| Trace export size | Storage footprint for L1 | n = 3 | Report bytes |

Use the same performance vocabulary as `docs/PERFORMANCE-ASSESSMENT.md` where
possible. Do not use n=3 latency values as a performance claim.

Emit wall-clock and RSS measurements in Bencher Metric Format (`BMF_JSON=1`
mode used by `scripts/perf_assess.sh`) so this experiment's overhead numbers
can slot into the existing Criterion/Bencher baseline rather than becoming a
one-off measurement.

## Field Matrix Output

The comparison script should produce a machine-readable JSON file and a
Markdown table.

| Field | L1 Trace | L2 Archive | Join Key | Claim Strength | Notes |
|---|---|---|---|---|---|
| `gen_ai.provider.name` | TODO | TODO | run | Positive observation | |
| `gen_ai.tool.call.id` | TODO | TODO | tool | Joinability | |
| `assay.archive.manifest_digest` | TODO | TODO | digest | Tamper-evident binding | |
| `capability_surface.filesystem_paths` | TODO | TODO | none | Bounded negative if healthy | |
| `observation_health.ringbuf_drops` | no | TODO | archive | Measurement integrity | |
| `gen_ai.usage.input_tokens` | TODO | no | span | Cost/context | |

## Claim Classes

| Claim | Trace Strength | Archive Strength | Joined Strength |
|---|---|---|---|
| "The agent reported a tool call to X" | Strong | Medium if SDK layer present | Strong |
| "The run opened path P" | Weak unless app-instrumented | Strong within healthy Linux/eBPF boundary | Stronger when joined to tool context |
| "The run did not access path P" | Not supported | Bounded negative if health is clean and path class is in capture scope | Bounded negative with trace context |
| "The model used N input tokens" | Strong if provider reported tokens | Not supported | Strong for trace, archive irrelevant |
| "The measurement was complete" | Not supported | Strong via health/correlation fields | Strong for archive side of the pair |
| "The trace and archive describe the same run" | Weak by `run_id` only | Weak by `run_id` only | Stronger with `assay.archive.manifest_digest` |
| "The trace and archive prove the agent was acceptable" | Not supported | Not supported | Not supported |

## Acceptance Criteria

v1 is complete when it produces:

- one deterministic workload definition;
- at least three dual-capture runs;
- one Runner archive per dual-capture run;
- one OTel/OpenInference trace export per dual-capture run;
- one normalized comparison JSON;
- one Markdown field matrix;
- one joinability table showing `gen_ai.tool.call.id` presence/absence;
- one manifest-digest binding check;
- one threats-to-validity section filled with observed caveats.

Hard gates:

- `observation_health.ringbuf_drops == 0`;
- kernel layer is complete;
- correlation status is clean;
- trace contains a recognizable agent/model/tool span shape;
- every dual-capture trace carries `assay.archive.manifest_digest`;
- the digest resolves to the archive's `manifest.json` bytes.

## Threats to Validity

- **Linux-only measurement.** Assay-Runner's measured-run claim currently relies
  on Linux/eBPF/cgroup v2. macOS and Windows require separate platform spikes.
- **Kernel-conditioned behavior.** Ring-buffer behavior and observable event
  shape can vary across kernel versions.
- **Instrumentation asymmetry.** OpenTelemetry-family traces depend on SDK and
  framework instrumentation. Missing fields may be configuration gaps, not
  conceptual limits.
- **Privacy gating.** Prompt, completion, tool arguments, and tool results are
  sensitive and may be intentionally disabled.
- **Streaming.** Streaming output may appear differently in spans, events, or
  SDK logs. v1 should not generalize beyond the chosen workload.
- **Statistical power.** n=3 is enough for determinism checks, not for overhead
  claims. Latency/RSS results require larger samples.
- **Generic kernel tracers omitted.** v1 does not compare against Tetragon,
  Falco, or Tracee. This is a scope decision, not a claim that L3 does not
  matter.
- **No acceptability claim.** Neither trace nor archive proves that behavior is
  safe or policy-acceptable without a separate policy interpretation layer.

## OpenInference Discussion Payload

After v1 data exists, open a focused discussion with a vocabulary question, not
a product pitch:

> We compared an OpenTelemetry/OpenInference trace with a content-addressed
> Assay-Runner measured-run archive for the same deterministic agent workload.
> The strongest join was a trace attribute/event that points to the archive
> manifest by digest rather than embedding runtime evidence in the trace.
>
> Would OpenInference prefer a vocabulary like:
>
> - `agent.runtime_evidence.digest`
> - `agent.runtime_evidence.health`
> - `agent.runtime_evidence.boundary`
>
> or should this live under an existing OpenInference / OTel extension namespace?
> The goal is to let traces refer to measured-run artifacts without making the
> trace the semantic owner of that artifact.

Keep the discussion narrow. Ask for naming and domain placement, not adoption.

## Reproduction Checklist

Fill this section after implementation.

```bash
# TODO: build assay with runner feature
# TODO: run deterministic workload under Runner only
# TODO: run deterministic workload under OTel/OpenInference only
# TODO: run dual-capture workload
# TODO: extract manifest.json and compute digest
# TODO: generate normalized comparison JSON
# TODO: generate Markdown field matrix
```

## References to Verify Before Publication

- OpenTelemetry GenAI semantic conventions:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
- OpenTelemetry GenAI spans:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/>
- OpenTelemetry GenAI agent/framework spans:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/>
- OpenInference semantic conventions:
  <https://arize-ai.github.io/openinference/spec/semantic_conventions.html>
- SLSA provenance v1.1:
  <https://slsa.dev/spec/v1.1/provenance>
- Assay-Runner measured-run walkthrough:
  `docs/reference/runner/examples/measured-run-proof-bundle.md`
- AgentSight is referenced descriptively in the L2 row until a URL and title
  are verified. Do not add an arXiv ID without checking it first.

Do not include unverified arXiv IDs or synthetic-looking survey references in
the published version.
