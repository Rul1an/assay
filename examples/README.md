# Assay Examples

Ready-to-use examples to get started with Assay.

## Start here

### [MCP Quickstart](./mcp-quickstart)
Wrap an MCP server with policy enforcement in under 2 minutes.
See ALLOW/DENY decisions for every tool call.

## Evaluation examples

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

### [Pydantic AI Eval Report Evidence](./pydantic-ai-eval-report-evidence)
Map a tiny serialized artifact derived from a `pydantic_evals` `EvaluationReport` into Assay-shaped external evidence.
**Focus**: code-first eval-result seam, bounded case results only, no imported evaluator or tracing truth.

### [Agno Accuracy Eval Evidence](./agno-accuracy-evidence)
Map a tiny artifact derived from an Agno `AccuracyEval` / `AccuracyResult` surface into Assay-shaped external evidence.
**Focus**: eval-result-first seam, bounded scores and avg score only, no imported evaluator or tracing truth.

### [Browser Use History Evidence](./browser-use-history-evidence)
Map a tiny artifact derived from a Browser Use `AgentHistoryList` result surface into Assay-shaped external evidence.
**Focus**: history/output-first seam, bounded action-history reduction and final result only, no imported observability truth.

### [Visa TAP Verification Evidence](./tap-intent-evidence)
Map a tiny artifact derived from the Visa Trusted Agent Protocol signature-verification path into Assay-shaped external evidence.
**Focus**: verification-outcome-first seam, bounded signature metadata only, no imported payment or identity truth.

### [A2A Task Evidence](./a2a-task-evidence)
Map a tiny A2A task lifecycle export into Assay-shaped external evidence.
**Focus**: task-lifecycle-first seam, bounded route reference only, no imported trust or delegation truth.

### [UCP Checkout Evidence](./ucp-checkout-evidence)
Map a tiny UCP checkout/order lifecycle export into Assay-shaped external evidence.
**Focus**: checkout/order-state observation only, no imported payment, settlement, or merchant truth.

## 2. [Negation Safety](./negation-safety)
Ensure model adheres to critical safety instructions (e.g. "Do NOT").
**Metrics**: `regex`.

## 3. [Baseline Gate (CI)](./baseline-gate)
Full workflow demonstration of **Regression Testing** with Baselines.
**Features**: `--baseline`, `--export-baseline`.

## 4. [Python SDK Demo](./python-sdk-demo)
Native Python integration using `pytest` and `assay` library.
**Features**: `AssayClient`, `Coverage`, `pytest` integration.

## Usage
You can run any example directly from the root:

```bash
assay run --config examples/rag-grounding/eval.yaml --trace-file examples/rag-grounding/traces/good.jsonl
```
