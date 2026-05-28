# Claim-Boundary Positioning

> **Status:** reference positioning note. Last updated: 2026-05-28.
> This document does not open a new experiment arc, define a schema, or
> promote any `assay.experiment.*` artifact to a product API.

## Position

Assay is not an observability replacement, trace viewer, vendor ranking,
or general agent dashboard. Assay's strongest post-arc position is a
claim-boundary and evidence-fidelity layer for agent systems.

The closed Runner-vs-OTel overhead arc and agent-observability fidelity
arc support one shared statement:

> Assay helps determine what an agent run proves, not just what it
> emitted.

The useful boundary is not "Runner versus OTel" or "OpenInference versus
OTel GenAI." It is the boundary between reported intent, measured
effect, calibration health, join strength, and the claim a reviewer may
safely make.

## Methodology Anchor

Future agent-observability work should start from the pattern proven by
the two closed arcs:

1. **Calibrate before timing.** Requested signals must be compared with
   observed signals before throughput, timing, or absence claims are
   interpreted.
2. **Respect evidence boundaries.** In-process traces, OpenInference or
   OTel vocabularies, Runner archives, policy evidence, and kernel
   effects carry different claim surfaces.
3. **Classify claims.** A trace/archive mismatch is a measured
   divergence, not automatically malicious behavior, policy failure, or
   root cause.
4. **Carry bounded evidence.** A portable carrier should make review
   easier without strengthening the underlying evidence.
5. **Use delegated gates sparingly.** Real infrastructure should verify
   a specific publication gate, not broaden a synthetic experiment by
   accident.

This is the product-facing form of the experiment lifecycle documented
in [`../experiments/arc-lifecycle-guide.md`](../experiments/arc-lifecycle-guide.md).

## Adjacent Whitespace

This map is not a backlog of arcs. The experiment lifecycle guide makes
arc closure a stop condition, not permission to open every adjacent
question. The default next gate for a whitespace direction is an
upstream comment, a short research note, or a watch trigger. A new arc
requires a named consumer, upstream response, or concrete contract ask.

### 1. Protocol Tool-Evidence Assurance

**Question:** When a protocol-exposed tool is selected and invoked, can
Assay bind the tool metadata visible to the model, the tool call, policy
context, and measured runtime effect?

**Why now:** MCP has become a major tool-integration surface. The MCP
tools specification makes tools discoverable and model-controlled, while
recent work such as MCPTox and MCP-TDP shows that tool metadata and
descriptions are active attack surfaces.

**Assay angle:** Produce bounded evidence for "the model saw this tool
description, selected this tool, sent these arguments, and the system
measured these effects." Do not claim that every poisoned tool is
malicious or that every mismatch is an exploit.

**Lifecycle-conform next step:** write an upstream comment trilogy, not
an Assay arc:

- one comment on MCPTox or its follow-up discussion;
- one comment on MCP-TDP or its benchmark surface;
- one comment or issue on the MCP tools specification/discussion.

Each comment should reuse the same bounded evidence shape:
`tool_manifest_digest -> model-visible description -> tool_call ->
runtime_effect`. Do not promise an Assay schema, evidence-pack product,
or delegated experiment in those comments.

**Trigger to open an arc:** a maintainer, benchmark author, or downstream
consumer asks for concrete Assay evidence over a tool-poisoning case.

### 2. AgentSight Semantic-Gap Citation

**Question:** Can the closed fidelity arc cite an external formulation of
the same reported-intent versus measured-effect gap without reopening
the arc?

**Why now:** AgentSight explicitly frames the need to correlate
high-level agent intent with low-level system actions. That is the same
conceptual gap the fidelity arc made executable with semantic-gap
scenarios, join rows, and claim classes.

**Assay angle:** Use AgentSight as an external citation anchor for the
already-closed fidelity finding. Do not dispatch new `hidden_write` or
`path_rewrite` delegated captures just because the citation exists.

**Lifecycle-conform next step:** a narrow findings-summary citation
update or research note that says AgentSight names the same semantic-gap
class that Assay bounds through claim-strength and join evidence.

**Trigger to open an arc:** an external reader asks Assay to validate one
of the semantic-gap scenarios under a specific real agent framework.

### 3. Observability Fidelity SLOs

**Question:** Can "observability that looks cheap because it clipped the
requested signal" become a reusable category without reopening the OTel
span-limit study?

**Why now:** The overhead arc showed that the OTel
`SpanLimits.EventCountLimit=128` behavior is a configuration boundary,
not a throughput boundary. That makes it a clean example of a broader
fidelity-SLO problem: retention target, observed count, effective limit,
method, agreement, and safe claim class.

**Assay angle:** Turn Slice 12 into a vocabulary for observability
fidelity, not an OTel benchmark. The safe thesis is "measure fidelity
before timing," not "OTel is limited."

**Lifecycle-conform next step:** write a positioning blogpost or research
note. Keep issue #1408 trigger-only.

**Trigger to open an arc:** a concrete consumer asks "can Assay measure
this fidelity SLO for my trace path?"

### 4. Agent Identity And Delegation

**Question:** When an agent acts on behalf of a user or another agent,
what evidence would be needed to bind identity, delegation,
authorization, tool intent, and measured effect?

**Why now:** NIST's 2026 AI Agent Standards Initiative and AI-agent
identity work point at authentication, authorization, auditing, and
non-repudiation as open agent-system problems. Recent identity research
also highlights semantic intent verification and recursive delegation
accountability as unresolved gaps.

**Assay angle:** Treat identity and authorization as claim inputs, not
as generic metadata. A delegated action should be reviewable as "who or
what delegated the action, under which policy, through which key, and
with which measured effect."

**Lifecycle-conform next step:** one public comment on the NIST concept
paper or related standards discussion. The comment should introduce the
bounded-evidence vocabulary and then wait for response. Do not predeclare
a delegation receipt schema.

**Trigger to open an arc:** NIST, a standards participant, or a real
downstream consumer asks for a concrete Assay receipt shape.

### 5. Real Trace Interop Imports

**Question:** Given a real OTel GenAI or OpenInference trace and a
Runner proof pack for the same run, which claims map fully, partially,
or not at all?

**Why now:** OTel GenAI semantic conventions remain in Development, with
explicit opt-in transition behavior for latest experimental emission.
OpenInference uses a richer AI-specific span-kind vocabulary, including
`AGENT`, `LLM`, `TOOL`, `RETRIEVER`, `GUARDRAIL`, `EVALUATOR`, and
`PROMPT`.

**Assay angle:** The synthetic interop matrix is enough for now. OTel's
Development status is a reason to track the surface, not a reason to
build an importer before a stable target or consumer exists.

**Lifecycle-conform next step:** watch OTel GenAI and OpenInference
surface changes and keep the interop matrix citation-ready. Do not build
a translator or importer as a speculative product surface.

**Trigger to open an arc:** a stable upstream field surface or external
consumer asks Assay to map a specific real trace against Runner evidence.

### 6. Causal Claim Graphs

**Question:** Can Assay produce causal graph rows that state what is
known, weakly joined, absent, or inconclusive without asking an LLM to
invent root cause?

**Why now:** 2026 work on causal tracing, trace-to-graph analysis, and
multi-agent root-cause localization shows growing demand for structured
diagnosis over long agent traces. Assay can contribute a stricter
evidence-bounded graph layer.

**Assay angle:** The graph is not "the explanation." It is a typed view
of evidence, joins, health, calibration, and safe claim classes.

**Lifecycle-conform next step:** a research note only. Do not define
`causal_claim_cell.v0` or promote a graph schema until a concrete
consumer asks for graph output.

**Trigger to open an arc:** a paper, benchmark, or downstream integration
needs graph-shaped evidence rather than the existing join and claim-class
rows.

## Explicit Non-Directions

Do not build a public synthetic trace corpus now. A 20 to 50 run corpus
would require curation, hosting, contribution policy, and ongoing
maintenance. It also competes with AgentSim, Agentic CLEAR, and related
corpus work where Assay's advantage is not scale. Assay's advantage is
the narrowness of the claim and the evidence boundary around it.

Do not treat NIST identity work as an invitation to start a standards
arc. Standards engagement can become a time sink for a solo maintainer.
The right unit of work is one well-timed public comment, then waiting
for a concrete response.

Do not treat OTel GenAI Development status as an importer trigger. A
moving surface is a reason to track source snapshots and wait for a
stable or requested mapping target.

## Selection Rule

Open a new arc only when the question can be stated as:

> Which claim can we not safely make today because trace, protocol,
> policy, identity, or runtime evidence is not yet bound tightly enough?

Prefer future arcs that:

- create or validate a bounded claim class;
- bind two or more evidence layers that currently drift apart;
- make absence, partial coverage, clipping, or weak joins first-class;
- produce a carrier a reviewer can inspect;
- have a delegated baseline or a concrete downstream consumer.

Defer arcs that:

- mainly rank tools, vendors, vocabularies, or frameworks;
- add another dashboard without changing claim strength;
- expand a synthetic matrix without a new claim boundary;
- require schema promotion before a consumer exists;
- duplicate OTel/OpenInference trace viewing instead of binding traces
  to measured effects.

## Near-Term Priority

The near-term program is outreach and positioning, not new experiments:

1. **MCP comment trilogy.** One bounded-evidence comment each for
   MCPTox, MCP-TDP, and the MCP tools specification or discussion.
2. **AgentSight citation update.** Use AgentSight as an external
   formulation of the semantic gap already closed by the fidelity arc.
3. **Fidelity-SLO research note.** Reframe Slice 12 as "observability
   fidelity before timing" without opening #1408.
4. **NIST identity watch.** Make at most one public comment, then wait
   for response.
5. **Interop import watch.** Track OTel/OpenInference surface stability,
   but do not build a speculative importer.
6. **Causal graph watch.** Keep as a paper-tier possibility after a
   consumer asks for graph-shaped evidence.

## Source Anchors

- Assay overhead arc:
  [`../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md`](../../experiments/runner-vs-otel-overhead-2026-05/findings-summary.md)
- Assay fidelity arc:
  [`../../experiments/agent-observability-fidelity-2026-05/findings-summary.md`](../../experiments/agent-observability-fidelity-2026-05/findings-summary.md)
- Assay arc lifecycle:
  [`../experiments/arc-lifecycle-guide.md`](../experiments/arc-lifecycle-guide.md)
- OpenTelemetry GenAI semantic conventions:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
- OpenTelemetry GenAI agent spans:
  <https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/>
- OpenInference semantic conventions:
  <https://arize-ai.github.io/openinference/spec/semantic_conventions.html>
- Model Context Protocol tools:
  <https://modelcontextprotocol.io/specification/2025-06-18/server/tools>
- MCPTox:
  <https://ojs.aaai.org/index.php/AAAI/article/view/40895>
- MCP-TDP:
  <https://arxiv.org/abs/2605.24069>
- NIST AI Agent Standards Initiative:
  <https://www.nist.gov/artificial-intelligence/ai-agent-standards-initiative>
- NIST AI agent identity concept paper:
  <https://csrc.nist.gov/pubs/other/2026/02/05/accelerating-the-adoption-of-software-and-ai-agent/ipd>
- AI Identity survey:
  <https://arxiv.org/abs/2604.23280>
- AgentSight:
  <https://arxiv.org/abs/2508.02736>
- AgentTrace causal tracing:
  <https://arxiv.org/abs/2603.14688>
- AgentSim:
  <https://arxiv.org/abs/2604.26653>
- AgentGraph:
  <https://ojs.aaai.org/index.php/AAAI/article/view/42393>
- Agentic CLEAR:
  <https://ibm.github.io/CLEAR/>

## Non-Claims

- This document does not open the MCP/tool-evidence, identity,
  interop-import, corpus, causal-graph, or fidelity-SLO arcs.
- This document does not promote evidence packs, calibration verdicts,
  interop rows, semantic-gap verdicts, or future binding evidence to
  product APIs.
- This document does not rank OTel, OpenInference, MCP, A2A, Runner, or
  Assay as products.
- This document does not claim that every protocol, trace, or
  trace/kernel mismatch is a security issue.
