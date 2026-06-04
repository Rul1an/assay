# Assay Examples

Ready-to-use examples to get started with Assay.

## Start here

### [MCP Quickstart](./mcp-quickstart)
Wrap an MCP server with policy enforcement in under 2 minutes.
See ALLOW/DENY decisions for every tool call.

## Coverage honesty

How Assay keeps a coverage claim no stronger than the observation behind it —
capture, then a completeness ceiling, then claim cells, then enforcement and
aggregation. Every stage degrades rather than inflates when evidence is missing.
Start with the walkthrough; it links the runnable pieces in order.

### [Coverage Honesty Walkthrough](./coverage-honesty-walkthrough)
The whole chain in one place: capture → coverage descriptor → annotation →
enforcement → aggregation, plus the synthetic attestation-shape demonstrator.
A reading guide that points at each runnable example below.

### [Coverage-Aware Side-Effect Report](./coverage-aware-side-effect)
Single-archive sample that reads a runner archive and reports observed effects
with the coverage descriptor that bounds them.

### [Coverage-Aware Drift Annotation](./coverage-aware-drift-annotation)
Turn a cross-runtime drift report into honest claim cells (strength × basis),
capped by the coverage descriptor.

### [Coverage-Claims Gate](./coverage-claims-gate)
A dependency-free consumer that mechanically permits or blocks asserted coverage
claims against an annotation — enforcement, not just documentation.

### [Coverage Fleet Summary](./coverage-fleet-summary)
Fold many annotations into one fleet-level honesty summary, including the fleet
floor: the strongest positive claim supportable across *every* run.

### [Attested-Claim Composition Shape](./attested-shape-demo)
A clearly-labelled synthetic demonstrator of how a verifiable, subject-bound
claim would compose — degrading unless verified. No real attestation mechanism.

## Inventory examples

### [CycloneDX ML-BOM Model Component Evidence](./cyclonedx-mlbom-model-component-evidence)
Import one selected CycloneDX `machine-learning-model` component into a
verifiable Assay inventory receipt bundle.
**Focus**: model-component-first seam, bounded model identity and refs only,
no imported full BOM graph, modelCard body, dataset body, vulnerabilities, or
compliance truth.

## Evaluation examples

### [Phoenix Span Annotation Evidence](./phoenix-span-annotation-evidence)
Map a tiny artifact derived from Phoenix's span annotation retrieve path into
Assay-shaped external evidence.
**Focus**: annotation-first seam, bounded span-scoped feedback only, no
imported trace trees, experiments, or platform truth.

### [Mem0 Add Memories Evidence](./mem0-add-memories-evidence)
Map a tiny artifact derived from Mem0's `Add Memories` result path into
Assay-shaped external evidence.
**Focus**: mutation-result-first seam, bounded event labels and memory text
only, no imported search, graph, or profile truth.

### [AG-UI Compacted Message Snapshot Evidence](./ag-ui-compacted-message-snapshot-evidence)
Map a tiny artifact derived from one bounded AG-UI run envelope with one
compacted `MESSAGES_SNAPSHOT` seam into Assay-shaped external evidence.
**Focus**: compacted-message-history-first seam, bounded thread/run anchors and
text-bearing messages only, no imported state sync, replay, or full stream
truth.

### [Stagehand Selector-Scoped Extract Evidence](./stagehand-selector-scoped-extract-evidence)
Map a tiny artifact derived from one observe-derived selector plus one
selector-scoped extract result into Assay-shaped external evidence.
**Focus**: selector-scoped extraction-first seam, bounded selector anchor and
small structured result only, no imported snapshots, runtime truth, or full
observe planning truth.

### [OpenAI Agents JS Approval Interruption Evidence](./openai-agents-js-approval-interruption-evidence)
Map a tiny artifact derived from one paused OpenAI Agents JS approval run into
Assay-shaped external evidence.
**Focus**: approval-interruption-first seam, bounded `interruptions` and one
resumable continuation anchor only, no imported transcript, session, or full
`RunState` truth.

### [Vercel AI SDK UIMessage Evidence](./vercel-ai-uimessage-evidence)
Map a tiny artifact derived from Vercel AI SDK's `UIMessage` path into
Assay-shaped external evidence.
**Focus**: message-first seam, bounded text and tool parts only, no imported
traces, telemetry, or backend truth.

### [LlamaIndex EvaluationResult Evidence](./llamaindex-evalresult-evidence)
Map a tiny artifact derived from LlamaIndex's `EvaluationResult` path into
Assay-shaped external evidence.
**Focus**: eval-result-first seam, bounded pass/fail, score, and feedback
only, no imported traces, callbacks, or prompt truth.

### [LiveKit Agents Testing-Result Evidence](./livekit-runresult-evidence)
Map a tiny artifact derived from LiveKit Agents'
`voice.testing.RunResult.events` path into Assay-shaped external evidence.
**Focus**: testing-result-first seam, bounded typed run events only, no
imported telemetry, transcript, or room-state truth.

### [LiveKit Agents Tool Action Evidence](./livekit-tool-action-evidence)
Map a tiny artifact derived from LiveKit Agents'
`FunctionToolsExecutedEvent` surface into Assay-shaped external evidence.
**Focus**: acted-family candidate seam, bounded function call/output pairs
only, no imported transcript, audio, room state, usage telemetry, or trace
truth.

### [x402 Requirement / Verification Evidence](./x402-verification-evidence)
Map a tiny artifact derived from x402's `PaymentRequired` plus `VerifyResponse`
path into Assay-shaped external evidence.
**Focus**: requirement-and-verification-first seam, requirement-side amount and
asset context only, no imported settlement or fulfillment truth.

### [Mastra ScoreEvent Evidence](./mastra-score-event-evidence)
Map a tiny artifact derived from Mastra's `ObservabilityExporter` /
`ScoreEvent` path into Assay-shaped external evidence.
**Focus**: score-event-first seam, bounded `ExportedScore`-derived fields only,
no imported traces, dashboards, or broader observability truth.

### [Mastra Scorer Evidence](./mastra-scorer-evidence)
Map a tiny artifact derived from Mastra's earlier scorer / experiment seam
hypothesis into Assay-shaped external evidence.
**Focus**: scorer-result-first seam, bounded score and experiment context only,
kept for historical comparison with the newer score-event-first Mastra lane.

### [LangWatch Custom Span Evaluation Evidence](./langwatch-custom-span-evaluation-evidence)
Map a tiny artifact derived from LangWatch's custom `add_evaluation(...)` span
path into Assay-shaped external evidence.
**Focus**: surfaced child-evaluation-span seam, bounded pass/fail, score, label,
and optional details only, no imported trace, dataset, or evaluation-session truth.

### [AgentEvals Trajectory Strict-Match Evidence](./agentevals-trajectory-strict-match-evidence)
Map a tiny artifact derived from AgentEvals' deterministic strict-match
returned result into Assay-shaped external evidence.
**Focus**: returned-result-first seam, bounded evaluator key and boolean score
only, no imported trajectories, LangSmith wrappers, or evaluator-config truth.

### [AutoEvals ExactMatch Evidence](./autoevals-exactmatch-evidence)
Map a tiny artifact derived from AutoEvals' deterministic `ExactMatch` score
object into Assay-shaped external evidence.
**Focus**: returned-score-first seam, bounded scorer name and integer `0`/`1`
score only, no imported raw compared values, Braintrust wrappers, or scorer
config truth.

### [Guardrails Validation Outcome Evidence](./guardrails-validation-outcome-evidence)
Map a tiny artifact derived from Guardrails AI's direct `ValidationResult` path
into Assay-shaped external evidence.
**Focus**: validation-result-first seam, bounded pass/fail and short failure
message only, no imported raw output, corrected output, reask, or guard-history
truth.

### [OpenFeature EvaluationDetails Evidence](./openfeature-evaluation-details-evidence)
Map a tiny artifact derived from OpenFeature's detailed flag evaluation API
into Assay-shaped external evidence.
**Focus**: decision-detail-first surface, bounded flag key, returned value,
reason, variant, and fallback error code only, no imported provider config,
targeting, rollout, telemetry, or application correctness truth.

### [Promptfoo Assertion GradingResult Evidence](./promptfoo-assertion-grading-result-evidence)
Map a tiny artifact derived from Promptfoo's deterministic `equals` assertion
component result into Assay-shaped external evidence.
**Focus**: extracted-assertion-result-first seam, bounded pass/score/reason
only, no imported full eval exports, prompt matrices, provider responses, or
raw compared values.

### [RAG Grounding](./rag-grounding)
Evaluate if your RAG pipeline answers strictly based on context.
**Metrics**: `semantic_similarity`, `must_contain`, `must_not_contain`.

## Interop examples

### [Google ADK Evaluation Evidence](./adk-evaluation-evidence)
Map one tiny Google ADK evaluation artifact into Assay-shaped external evidence.
**Focus**: evaluation/artifact-first seam, trajectory as observed reference only, no imported evaluator truth.

### [AGT Audit Evidence](./agt-audit-evidence)
Map a tiny AGT `mcp-trust-proxy`-style audit corpus into Assay-shaped external evidence.
**Focus**: allow/deny audit decisions, malformed import failure, no imported trust semantics.

### [CrewAI Event Evidence](./crewai-event-evidence)
Export a small CrewAI event-listener artifact and map it into Assay-shaped external evidence.
**Focus**: bounded task/tool events, optional MCP bonus path, no imported trust semantics.

### [LangGraph Task Evidence](./langgraph-task-evidence)
Export a tiny LangGraph `tasks` v2 stream artifact and map it into Assay-shaped external evidence.
**Focus**: OSS-native tasks seam hypothesis, minimal checkpointer dependency, no imported orchestration truth.

### [OpenAI Agents Trace Evidence](./openai-agents-trace-evidence)
Export a tiny OpenAI Agents trace artifact through a local custom `TraceProcessor`.
**Focus**: trace-processor-first seam, bounded local export, no imported runtime truth.

### [Microsoft Agent Framework Trace Evidence](./maf-trace-evidence)
Map a tiny Microsoft Agent Framework exported trace artifact into Assay-shaped external evidence.
**Focus**: exported OpenTelemetry trace seam, bounded span metadata only, no imported runtime or governance truth.

### [MCP-Agent Token Evidence](./mcp-agent-token-evidence)
Map a tiny `mcp-agent` token-summary artifact into Assay-shaped external evidence.
**Focus**: bounded runtime-accounting seam, upstream cost estimate only, no imported billing or workflow truth.

### [MCP Tunnel Observed-Facts Evidence](./mcp-tunnel-observed-evidence)
Map a tiny provider-neutral MCP tunnel observation artifact into Assay-shaped
external evidence.
**Focus**: tunnel-observed route/upstream facts plus request-envelope binding
only, no imported tunnel trust, auth correctness, policy outcome, or tool truth.

### [Coverage-Aware Side-Effect Report](./coverage-aware-side-effect)
Turn a Runner archive's `observation_health` and `capability_surface` into
per-dimension claim cells plus blocked claims, reusing the shipped
`coverage_descriptor.v0` claim-kind gate.
**Focus**: positive existence is strong measured, exhaustive set is weak under
declared blind spots, and bounded-negative (absence) claims are blocked unless
coverage is complete and capture is clean. Derived report only, no schema change.

### [Pydantic Evals Reduced Case-Result Evidence](./pydantic-ai-eval-report-evidence)
Map one reduced artifact derived from `pydantic_evals` `EvaluationReport.cases[]` into Assay-shaped external evidence or importer-only Pydantic case-result receipts.
**Focus**: code-first case-result seam, no raw `ReportCase`, report summary, task input/output, tracing truth, or Trust Basis claim.

### [Agno Accuracy Eval Evidence](./agno-accuracy-evidence)
Map a tiny artifact derived from an Agno `AccuracyEval` / `AccuracyResult` surface into Assay-shaped external evidence.
**Focus**: eval-result-first seam, bounded scores and avg score only, no imported evaluator or tracing truth.

### [Browser Use History Evidence](./browser-use-history-evidence)
Map a tiny artifact derived from a Browser Use `AgentHistoryList` result surface into Assay-shaped external evidence.
**Focus**: history/output-first seam, bounded action-history reduction and final result only, no imported observability truth.

### [Visa TAP Verification Evidence](./tap-intent-evidence)
Map a tiny artifact derived from the Visa Trusted Agent Protocol signature-verification path into Assay-shaped external evidence.
**Focus**: verification-outcome-first seam, bounded signature metadata only, no imported payment or identity truth.

### [Langfuse Experiment Result Evidence](./langfuse-experiment-evidence)
Map a tiny artifact derived from the Langfuse experiment runner path into Assay-shaped external evidence.
**Focus**: experiment-result-first seam, bounded item output and evaluations only, no imported trace or dashboard truth.

### [A2A Task Evidence](./a2a-task-evidence)
Map a tiny A2A task lifecycle export into Assay-shaped external evidence.
**Focus**: task-lifecycle-first seam, bounded route reference only, no imported trust or delegation truth.

### [UCP Checkout Evidence](./ucp-checkout-evidence)
Map a tiny UCP checkout/order lifecycle export into Assay-shaped external evidence.
**Focus**: checkout/order-state observation only, no imported payment, settlement, or merchant truth.

### [Coverage-Aware Drift Annotation](./coverage-aware-drift-annotation)
Attach per-dimension claim cells to a `runtime_drift.v0.2` report so a full-overlap drift row is not read as exhaustive-equality or bounded-negative.
**Focus**: comparator-output coverage ceiling, blocked absence claims, no comparator or schema change.

## 2. [Negation Safety](./negation-safety)
Ensure model adheres to critical safety instructions (e.g. "Do NOT").
**Metrics**: `regex`.

## 3. [Baseline Gate (CI)](./baseline-gate)
Full workflow demonstration of **Regression Testing** with Baselines.
**Features**: `--baseline`, `--export-baseline`.

## 4. [Python SDK Docs](../docs/python-sdk/)
Native Python integration using `pytest` and the `assay` library.
**Features**: `AssayClient`, `Coverage`, `pytest` integration.

## Usage
You can run any example directly from the root:

```bash
assay run --config examples/rag-grounding/eval.yaml --trace-file examples/rag-grounding/traces/good.jsonl
```
